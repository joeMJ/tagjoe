//! Background work on plain threads; results arrive as messages through a channel.

use crate::Message;
use iced::futures::channel::mpsc;
use iced::Task;
use std::path::PathBuf;
use std::sync::Arc;
use tagjoe_core::apply::{apply_with_progress, undo, ApplyResult};
use tagjoe_core::config::{journal_dir, Config};
use tagjoe_core::journal::Journal;
use tagjoe_core::model::{Kind, Unit};
use tagjoe_core::propose::Plan;
use tagjoe_core::scan;
use tagjoe_core::sources::books::BookIdentifier;
use tagjoe_core::sources::http::Http;
use tagjoe_core::sources::identify::{Identified, Identifier};

pub fn http(cfg: &Config) -> Http {
    Http::new(tagjoe_core::USER_AGENT, Some(tagjoe_core::config::cache_dir().join("http")), cfg.sources.cache_days)
}

/// Scan a dropped folder: one message per unit found.
pub fn scan_task(path: PathBuf) -> Task<Message> {
    let (tx, rx) = mpsc::unbounded::<Message>();
    std::thread::spawn(move || {
        match scan::scan_path(&path) {
            Ok(units) => {
                if units.is_empty() {
                    let _ = tx.unbounded_send(Message::Status(format!("No audio files in {}", path.display())));
                }
                for u in units {
                    let _ = tx.unbounded_send(Message::Scanned(Arc::new(u)));
                }
            }
            Err(e) => {
                let _ = tx.unbounded_send(Message::Status(format!("{e}")));
            }
        }
    });
    Task::run(rx, |m| m)
}

/// Identify a unit online (music or audiobook); fetches covers for the candidates.
pub fn identify_task(job: usize, unit: Arc<Unit>, cfg: Arc<Config>, offline: bool) -> Task<Message> {
    let (tx, rx) = mpsc::unbounded::<Message>();
    std::thread::spawn(move || {
        let mut http = http(&cfg);
        http.offline = offline;
        let progress_tx = tx.clone();
        let progress = move |step: &str, done: usize, total: usize| {
            let _ = progress_tx.unbounded_send(Message::Progress(job, format!("{step} {done}/{total}")));
        };
        let result: Result<Vec<Identified>, String> = if offline {
            Ok(vec![])
        } else if unit.kind == Kind::Audiobook {
            BookIdentifier { http: &http, cfg: &cfg }.identify(&unit, 3, &progress).map_err(|e| e.to_string())
        } else {
            Identifier { http: &http, cfg: &cfg }.identify(&unit, 3, &progress).map_err(|e| e.to_string())
        };
        let result = result.map(|mut cands| {
            let ident = Identifier { http: &http, cfg: &cfg };
            for (i, c) in cands.iter_mut().enumerate() {
                if c.release.score >= 30 || i == 0 {
                    let _ = ident.fetch_cover(&mut c.release);
                }
            }
            cands
        });
        let _ = tx.unbounded_send(Message::Identified(job, result));
    });
    Task::run(rx, |m| m)
}

/// Apply a plan on a thread; one `ApplyProgress` per written or deleted file, then `Applied`.
pub fn apply_task(job: usize, plan: Plan, cfg: Arc<Config>) -> Task<Message> {
    let (tx, rx) = mpsc::unbounded::<Message>();
    std::thread::spawn(move || {
        let ptx = tx.clone();
        let r = apply_with_progress(&plan, &cfg, &journal_dir(), &mut |p| {
            let _ = ptx.unbounded_send(Message::ApplyProgress(job, p.clone()));
        });
        let _ = tx.unbounded_send(Message::Applied(job, r));
    });
    Task::run(rx, |m| m)
}

pub fn undo_task(cfg: Arc<Config>) -> Task<Message> {
    Task::perform(
        async move {
            let (tx, rx) = iced::futures::channel::oneshot::channel::<Result<(String, ApplyResult), String>>();
            std::thread::spawn(move || {
                let jdir = journal_dir();
                let r = (|| {
                    let latest = Journal::list(&jdir).into_iter().next().ok_or_else(|| "no journal to undo".to_string())?;
                    let j = Journal::load(&latest).map_err(|e| e.to_string())?;
                    let res = undo(&j, &cfg);
                    if res.errors == 0 {
                        let _ = std::fs::rename(&latest, latest.with_extension("json.undone"));
                    }
                    Ok((j.unit.display().to_string(), res))
                })();
                let _ = tx.send(r);
            });
            rx.await.unwrap_or_else(|_| Err("undo thread failed".into()))
        },
        Message::Undone,
    )
}

/// Fetch cover bytes from a URL (manual cover entry).
pub fn fetch_url_task(job: usize, url: String, cfg: Arc<Config>) -> Task<Message> {
    Task::perform(
        async move {
            let (tx, rx) = iced::futures::channel::oneshot::channel::<Result<Vec<u8>, String>>();
            std::thread::spawn(move || {
                let http = http(&cfg);
                let r = http.get_bytes(&url, &[], "img").map_err(|e| e.to_string());
                let _ = tx.send(r);
            });
            rx.await.unwrap_or_else(|_| Err("fetch thread failed".into()))
        },
        move |r| Message::CoverFetched(job, r),
    )
}

/// Last journal file name (for the footer).
pub fn last_journal() -> Option<String> {
    Journal::list(&journal_dir()).into_iter().next().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}

/// Native folder dialog (XDG portal on Linux → the desktop's own file chooser, SMB shares
/// included; native dialogs on Windows and macOS).
pub fn pick_folder_task(start: Option<PathBuf>) -> Task<Message> {
    Task::perform(
        async move {
            let mut d = rfd::AsyncFileDialog::new().set_title("tagjoe – choose a folder");
            if let Some(s) = start.filter(|p| p.is_dir()) {
                d = d.set_directory(s);
            }
            d.pick_folder().await.map(|h| h.path().to_path_buf())
        },
        Message::FolderPicked,
    )
}

/// Native file dialog for a cover image.
pub fn pick_image_task(job: usize, start: Option<PathBuf>) -> Task<Message> {
    Task::perform(
        async move {
            let mut d = rfd::AsyncFileDialog::new().set_title("tagjoe – choose a cover image").add_filter("Images", &["jpg", "jpeg", "png"]);
            if let Some(s) = start.filter(|p| p.is_dir()) {
                d = d.set_directory(s);
            }
            d.pick_file().await.map(|h| h.path().to_path_buf())
        },
        move |p| Message::CoverPicked(job, p),
    )
}

/// Ask GitHub for a newer release (only when the user switched the check on or clicks "Check now").
pub fn update_check_task(manual: bool) -> Task<Message> {
    Task::perform(
        async move {
            let (tx, rx) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(tagjoe_core::update::check());
            });
            rx.await.unwrap_or_else(|_| Err("update thread failed".into()))
        },
        move |r| Message::UpdateChecked(manual, r),
    )
}

/// Download the package of a release into the cache and verify it against SHA256SUMS.
pub fn update_download_task(release: tagjoe_core::update::Release) -> Task<Message> {
    Task::perform(
        async move {
            let (tx, rx) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let dir = tagjoe_core::config::cache_dir().join("updates");
                let _ = tx.send(tagjoe_core::update::download(&release, &dir));
            });
            rx.await.unwrap_or_else(|_| Err("download thread failed".into()))
        },
        Message::UpdateDownloaded,
    )
}

/// Download fpcalc (Chromaprint) into tagjoe's tools folder, after the user clicked the button.
pub fn fpcalc_download_task() -> Task<Message> {
    Task::perform(
        async move {
            let (tx, rx) = iced::futures::channel::oneshot::channel();
            std::thread::spawn(move || {
                let _ = tx.send(tagjoe_core::sources::fingerprint::download_fpcalc());
            });
            rx.await.unwrap_or_else(|_| Err("download thread failed".into()))
        },
        Message::FpcalcDownloaded,
    )
}
