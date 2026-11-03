//! tagjoe desktop application (iced).
#![forbid(unsafe_code)]

mod i18n;
mod settings;
mod theme;
mod view;
mod work;

use iced::widget::{image, pane_grid};
use std::collections::BTreeSet;
use iced::{event, window, Element, Subscription, Task, Theme};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tagjoe_core::apply::{ApplyResult, Progress};
use tagjoe_core::config::Config;
use tagjoe_core::model::{Field, Kind, Unit};
use tagjoe_core::propose::{self, CoverSource, Plan, ReleaseInfo};
use tagjoe_core::schema::Status;
use tagjoe_core::sources::identify::Identified;

#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Scanning,
    Identifying(String),
    Ready,
    Applying,
    Done(String),
    Failed(String),
}

/// One folder in the queue.
pub struct Job {
    pub unit: Arc<Unit>,
    pub state: JobState,
    pub candidates: Vec<Identified>,
    pub chosen: Option<usize>,
    pub plan: Option<Plan>,
    pub selected_file: usize,
    pub album_overrides: BTreeMap<Field, String>,
    pub file_overrides: BTreeMap<(usize, Field), String>,
    pub cover_override: Option<CoverSource>,
    pub cover_handle: Option<image::Handle>,
    pub notes: Vec<String>,
    pub cover_input: String,
    pub result: Option<ApplyResult>,
    /// Files whose rename is switched off.
    pub rename_off: BTreeSet<usize>,
    /// (file, field) changes the user unchecked.
    pub skipped: BTreeSet<(usize, Field)>,
    /// (file, removal index) the user unchecked.
    pub skipped_removals: BTreeSet<(usize, usize)>,
    /// Files whose cover embedding is switched off.
    pub cover_off: BTreeSet<usize>,
    /// Header switch: embed the cover into every file (per-file boxes below are exceptions).
    pub cover_all: bool,
    /// Folder files (index into `plan.delete_files`) the user keeps.
    pub delete_off: BTreeSet<usize>,
    /// Progress of the running apply: (done, total, current file).
    pub progress: Option<(usize, usize, String)>,
}

/// One line of the activity log shown in the footer panel.
#[derive(Debug, Clone)]
pub struct LogLine {
    pub time: String,
    pub unit: String,
    pub file: String,
    pub action: String,
    pub ok: bool,
}

impl LogLine {
    fn parse(line: &str) -> Option<LogLine> {
        let mut it = line.splitn(4, '\t');
        let time = it.next()?.to_string();
        let unit = it.next()?.to_string();
        let file = it.next()?.to_string();
        let action = it.next().unwrap_or_default().to_string();
        let ok = !(action.starts_with("error") || action.starts_with("Fehler"));
        Some(LogLine { time, unit, file, action, ok })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Files,
    Fields,
}

pub struct App {
    pub cfg: Arc<Config>,
    pub theme: Theme,
    pub jobs: Vec<Job>,
    pub selected: Option<usize>,
    pub path_input: String,
    pub status: String,
    pub settings: Option<settings::SettingsState>,
    pub offline: bool,
    pub last_journal: Option<String>,
    pub panes: pane_grid::State<Pane>,
    /// Activity log, newest first (this session plus the tail of log.txt).
    pub log: Vec<LogLine>,
    pub log_open: bool,
    /// Update banner state.
    pub update: Option<UpdateState>,
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    Checking,
    Offered(tagjoe_core::update::Release),
    Downloading(tagjoe_core::update::Release),
    /// Verified package, ready to install with the shown command.
    Ready(tagjoe_core::update::Release, PathBuf),
    Failed(String),
    /// Manual check without a newer version.
    UpToDate,
}

#[derive(Debug, Clone)]
pub enum Message {
    SystemTheme(iced::theme::Mode),
    PathInput(String),
    AddPath,
    Dropped(PathBuf),
    Scanned(Arc<Unit>),
    Progress(usize, String),
    Identified(usize, Result<Vec<Identified>, String>),
    SelectJob(usize),
    RemoveJob(usize),
    SelectFile(usize),
    ChooseCandidate(usize, usize),
    SetKind(usize, Kind),
    ToggleChange(usize, Field, bool),
    ToggleRemoval(usize, usize, bool),
    ToggleCover(usize, bool),
    ToggleRename(usize, bool),
    EditNew(usize, Field, String),
    EditAlbum(usize, Field, String),
    CoverInput(usize, String),
    CoverUse(usize),
    CoverFetched(usize, Result<Vec<u8>, String>),
    CoverFromFolder(usize),
    Apply(usize),
    CleanOnly(usize),
    ApplyProgress(usize, Progress),
    Applied(usize, ApplyResult),
    ToggleLog,
    ClearLog,
    ToggleCoverAll(usize, bool),
    ToggleDelete(usize, usize, bool),
    Undo,
    Undoned,
    Undone(Result<(String, ApplyResult), String>),
    OpenSettings,
    Settings(settings::SettingsMessage),
    Status(String),
    ToggleOffline(bool),
    ToggleRenameAll(usize, bool),
    PaneResized(pane_grid::ResizeEvent),
    OpenFolderPicker,
    FolderPicked(Option<PathBuf>),
    OpenCoverPicker(usize),
    CoverPicked(usize, Option<PathBuf>),
    OpenUrl(String),
    CheckUpdate(bool),
    UpdateChecked(bool, Result<Option<tagjoe_core::update::Release>, String>),
    UpdateDownload,
    UpdateDownloaded(Result<PathBuf, String>),
    UpdateInstall,
    UpdateDismiss,
    FpcalcDownloaded(Result<PathBuf, String>),
}

/// Command line: `tagjoe-gui [--theme light|dark] [--offline] [FOLDER…]`
struct Args {
    theme: Option<String>,
    offline: bool,
    folders: Vec<PathBuf>,
}

fn args() -> Args {
    let mut a = Args { theme: None, offline: false, folders: vec![] };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--theme" => a.theme = it.next(),
            "--offline" => a.offline = true,
            "--help" | "-h" => {
                println!("tagjoe-gui [--theme light|dark] [--offline] [FOLDER…]");
                std::process::exit(0);
            }
            _ => a.folders.push(PathBuf::from(x)),
        }
    }
    a
}

fn boot() -> (App, Task<Message>) {
    let a = args();
    let cfg = Config::load();
    tagjoe_core::i18n::init_from_env(&cfg.language);
    let panes = pane_grid::State::with_configuration(pane_grid::Configuration::Split { axis: pane_grid::Axis::Horizontal, ratio: 0.5, a: Box::new(pane_grid::Configuration::Pane(Pane::Files)), b: Box::new(pane_grid::Configuration::Pane(Pane::Fields)) });
    let app = App { cfg: Arc::new(cfg), theme: if a.theme.as_deref() == Some("dark") { theme::dark() } else { theme::light() }, jobs: vec![], selected: None, path_input: String::new(), status: i18n::t("Drop a folder onto the window or enter a path."), settings: None, offline: a.offline, last_journal: work::last_journal(), panes, log: load_log(), log_open: false, update: None };
    let mut tasks = Vec::new();
    // automatic check: only when switched on, at most once a day
    if app.cfg.update_check && tagjoe_core::update::now().saturating_sub(app.cfg.update_last_check) >= tagjoe_core::update::CHECK_INTERVAL_SECS {
        tasks.push(work::update_check_task(false));
    }
    if a.theme.is_none() {
        tasks.push(iced::system::theme().map(Message::SystemTheme));
    }
    for f in a.folders {
        tasks.push(work::scan_task(f));
    }
    (app, Task::batch(tasks))
}

/// The tail of log.txt, newest first.
fn load_log() -> Vec<LogLine> {
    let mut v: Vec<LogLine> = tagjoe_core::log::tail(500).iter().filter_map(|l| LogLine::parse(l)).collect();
    v.reverse();
    v
}

impl App {
    /// Record an action in the panel and in log.txt.
    fn log(&mut self, unit: &str, file: &str, action: &str, ok: bool) {
        let line = tagjoe_core::log::line(unit, file, action);
        let _ = tagjoe_core::log::append(std::slice::from_ref(&line));
        if let Some(mut l) = LogLine::parse(&line) {
            l.ok = ok;
            self.log.insert(0, l);
            self.log.truncate(tagjoe_core::log::MAX_LINES);
        }
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        event::listen_with(|e, _status, _id| match e {
            iced::Event::Window(window::Event::FileDropped(p)) => Some(Message::Dropped(p)),
            _ => None,
        }),
        iced::system::theme_changes().map(Message::SystemTheme),
    ])
}

fn cover_handle(src: &CoverSource) -> Option<image::Handle> {
    match src {
        CoverSource::Bytes { bytes, .. } => Some(image::Handle::from_bytes(bytes.clone())),
        CoverSource::File { path } => Some(image::Handle::from_path(path)),
        CoverSource::Url { .. } => None,
    }
}

impl Job {
    fn new(unit: Arc<Unit>) -> Self {
        Job { unit, state: JobState::Scanning, candidates: vec![], chosen: None, plan: None, selected_file: 0, album_overrides: BTreeMap::new(), file_overrides: BTreeMap::new(), cover_override: None, cover_handle: None, notes: vec![], cover_input: String::new(), result: None, rename_off: BTreeSet::new(), skipped: BTreeSet::new(), skipped_removals: BTreeSet::new(), cover_off: BTreeSet::new(), cover_all: true, delete_off: BTreeSet::new(), progress: None }
    }

    /// Reference data + matching for the current choice (candidate or tags/offline).
    fn reference(&self, cfg: &Config) -> (ReleaseInfo, Vec<Option<usize>>) {
        if let Some(c) = self.chosen.and_then(|i| self.candidates.get(i)) {
            let mut rel = c.release.clone();
            if rel.cover.is_none() {
                rel.cover = self.unit.cover_files.first().map(|p| CoverSource::File { path: p.clone() });
            }
            return (rel, c.matched.clone());
        }
        if self.unit.kind == Kind::Audiobook {
            tagjoe_core::sources::books::offline_release(&self.unit, cfg)
        } else {
            (propose::release_from_unit(&self.unit), propose::identity_match(self.unit.files.len()))
        }
    }

    /// Build the plan from the reference and the user's manual edits.
    pub fn rebuild(&mut self, cfg: &Config) {
        let (mut rel, matched) = self.reference(cfg);
        for (f, v) in &self.album_overrides {
            rel.album.insert(*f, v.clone());
        }
        for ((i, f), v) in &self.file_overrides {
            if let Some(t) = matched.get(*i).copied().flatten().and_then(|t| rel.tracks.get_mut(t)) {
                t.fields.insert(*f, v.clone());
            } else if !f.is_album_level() {
                // unmatched file: extend tracks so the override has a place
                while rel.tracks.len() <= *i {
                    rel.tracks.push(Default::default());
                }
                rel.tracks[*i].fields.insert(*f, v.clone());
            }
        }
        let matched: Vec<Option<usize>> = (0..self.unit.files.len()).map(|i| matched.get(i).copied().flatten().or(if self.file_overrides.keys().any(|(fi, _)| *fi == i) { Some(i) } else { None })).collect();
        if let Some(c) = &self.cover_override {
            rel.cover = Some(c.clone());
        }
        self.cover_handle = rel.cover.as_ref().and_then(cover_handle);
        let mut plan = propose::build_plan(&self.unit, &rel, &matched, cfg);
        // manual values always apply; the user's unchecked boxes survive a rebuild
        for (i, fp) in plan.files.iter_mut().enumerate() {
            for ch in fp.changes.iter_mut() {
                let manual = self.album_overrides.contains_key(&ch.field) || self.file_overrides.contains_key(&(i, ch.field));
                if manual && ch.status != Status::Ok {
                    ch.apply = true;
                }
                if self.skipped.contains(&(i, ch.field)) {
                    ch.apply = false;
                }
            }
            for (ri, r) in fp.removals.iter_mut().enumerate() {
                if self.skipped_removals.contains(&(i, ri)) {
                    r.apply = false;
                }
            }
            if self.rename_off.contains(&i) {
                fp.new_name = None;
            }
            if let Some(c) = fp.cover.as_mut() {
                if !self.cover_all || self.cover_off.contains(&i) {
                    c.apply = false;
                }
            }
        }
        for (i, d) in plan.delete_files.iter_mut().enumerate() {
            if self.delete_off.contains(&i) {
                d.apply = false;
            }
        }
        self.plan = Some(plan);
    }

    /// Where the proposed cover comes from and how large it is, for the header line.
    pub fn cover_origin(&self) -> CoverOrigin {
        let plan = self.plan.as_ref();
        let change = plan.and_then(|p| p.files.iter().find_map(|f| f.cover.as_ref()));
        let Some(c) = change else {
            return if self.unit.files.iter().any(|f| f.has_cover()) { CoverOrigin::EmbeddedOnly } else { CoverOrigin::None };
        };
        let dims = (c.width, c.height);
        match &c.source {
            CoverSource::File { path } => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if self.unit.cover_files.iter().any(|p| p == path) { CoverOrigin::Folder(name, dims) } else { CoverOrigin::Picked(name, dims) }
            }
            CoverSource::Bytes { .. } => {
                let src = self.chosen.and_then(|i| self.candidates.get(i)).map(|c| c.release.source.clone()).unwrap_or_default();
                if self.cover_override.is_some() {
                    CoverOrigin::Url(dims)
                } else if src.to_lowercase().contains("discogs") {
                    CoverOrigin::Discogs(dims)
                } else {
                    CoverOrigin::CoverArtArchive(dims)
                }
            }
            CoverSource::Url { .. } => CoverOrigin::Url(dims),
        }
    }

    pub fn summary(&self) -> (usize, usize, usize, usize, usize) {
        let Some(p) = &self.plan else { return (0, 0, 0, 0, 0) };
        let complete = p.files.iter().filter(|f| f.changes.iter().all(|c| c.status == Status::Ok || !c.apply && c.status == Status::Missing && c.new.is_empty()) && f.removals.is_empty()).count();
        let incomplete = p.files.len() - complete;
        let foreign: usize = p.files.iter().map(|f| f.removals.len()).sum();
        let no_cover = p.files.iter().filter(|f| f.cover.as_ref().map(|c| c.apply).unwrap_or(false) || (!self.unit.files.iter().any(|x| x.has_cover()) && f.cover.is_none())).count();
        let renames = p.files.iter().filter(|f| f.new_name.is_some()).count();
        (complete, incomplete, foreign, no_cover, renames)
    }
}

/// Origin of the proposed cover (header line of the release card).
#[derive(Debug, Clone, PartialEq)]
pub enum CoverOrigin {
    None,
    /// No new cover; some files carry one already.
    EmbeddedOnly,
    Folder(String, (u32, u32)),
    Picked(String, (u32, u32)),
    Url((u32, u32)),
    CoverArtArchive((u32, u32)),
    Discogs((u32, u32)),
}

fn update(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::SystemTheme(mode) => {
            if args().theme.is_none() {
                app.theme = if mode == iced::theme::Mode::Dark { theme::dark() } else { theme::light() };
            }
        }
        Message::PathInput(s) => app.path_input = s,
        Message::AddPath => {
            let p = PathBuf::from(app.path_input.trim());
            if p.is_dir() {
                app.path_input.clear();
                app.status = format!("{} {}", i18n::t("Scanning"), p.display());
                return work::scan_task(p);
            }
            app.status = i18n::t("Not a folder.");
        }
        Message::Dropped(p) => {
            let p = if p.is_file() { p.parent().map(PathBuf::from).unwrap_or(p) } else { p };
            if p.is_dir() {
                app.status = format!("{} {}", i18n::t("Scanning"), p.display());
                return work::scan_task(p);
            }
        }
        Message::Scanned(unit) => {
            if app.jobs.iter().any(|j| j.unit.path == unit.path) {
                app.status = format!("{}: {}", i18n::t("Already in the queue"), unit.path.display());
                return Task::none();
            }
            let mut job = Job::new(unit.clone());
            job.rebuild(&app.cfg);
            job.state = if app.offline { JobState::Ready } else { JobState::Identifying(i18n::t("starting")) };
            app.jobs.push(job);
            let idx = app.jobs.len() - 1;
            if app.selected.is_none() {
                app.selected = Some(idx);
            }
            app.status = format!("{}: {}", i18n::t("Added"), unit.name());
            if !app.offline {
                return work::identify_task(idx, unit, app.cfg.clone(), false);
            }
        }
        Message::Progress(job, s) => {
            if let Some(j) = app.jobs.get_mut(job) {
                if matches!(j.state, JobState::Identifying(_)) {
                    j.state = JobState::Identifying(s);
                }
            }
        }
        Message::Identified(job, result) => {
            if let Some(j) = app.jobs.get_mut(job) {
                match result {
                    Ok(cands) => {
                        j.notes = cands.first().map(|c| c.notes.clone()).unwrap_or_default();
                        j.chosen = cands.first().filter(|c| c.release.score >= 50).map(|_| 0);
                        j.candidates = cands;
                        if let Some(c) = j.chosen.and_then(|i| j.candidates.get(i)) {
                            let kind = tagjoe_core::sources::identify::kind_from_release(&c.release, j.unit.kind);
                            if kind != j.unit.kind {
                                let mut u = (*j.unit).clone();
                                u.kind = kind;
                                j.unit = Arc::new(u);
                            }
                        }
                        j.rebuild(&app.cfg);
                        j.state = JobState::Ready;
                    }
                    Err(e) => {
                        j.notes.push(e);
                        j.state = JobState::Ready;
                    }
                }
            }
        }
        Message::SelectJob(i) => {
            if i < app.jobs.len() {
                app.selected = Some(i);
            }
        }
        Message::RemoveJob(i) => {
            if i < app.jobs.len() && !matches!(app.jobs[i].state, JobState::Applying) {
                app.jobs.remove(i);
                app.selected = if app.jobs.is_empty() { None } else { Some(app.selected.map(|s| if s >= app.jobs.len() { app.jobs.len() - 1 } else if s > i { s - 1 } else { s }).unwrap_or(0)) };
            }
        }
        Message::SelectFile(i) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                j.selected_file = i;
            }
        }
        Message::ChooseCandidate(job, c) => {
            if let Some(j) = app.jobs.get_mut(job) {
                j.chosen = if c < j.candidates.len() { Some(c) } else { None };
                j.rebuild(&app.cfg);
            }
        }
        Message::SetKind(job, kind) => {
            if let Some(j) = app.jobs.get_mut(job) {
                let mut u = (*j.unit).clone();
                u.kind = kind;
                j.unit = Arc::new(u);
                j.rebuild(&app.cfg);
            }
        }
        Message::ToggleChange(file, field, on) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                if on {
                    j.skipped.remove(&(file, field));
                } else {
                    j.skipped.insert((file, field));
                }
                if let Some(c) = j.plan.as_mut().and_then(|p| p.files.get_mut(file)).and_then(|f| f.changes.iter_mut().find(|c| c.field == field)) {
                    c.apply = on;
                }
            }
        }
        Message::ToggleRemoval(file, idx, on) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                if on {
                    j.skipped_removals.remove(&(file, idx));
                } else {
                    j.skipped_removals.insert((file, idx));
                }
                if let Some(r) = j.plan.as_mut().and_then(|p| p.files.get_mut(file)).and_then(|f| f.removals.get_mut(idx)) {
                    r.apply = on;
                }
            }
        }
        Message::ToggleCover(file, on) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                if on {
                    j.cover_off.remove(&file);
                } else {
                    j.cover_off.insert(file);
                }
                if let Some(c) = j.plan.as_mut().and_then(|p| p.files.get_mut(file)).and_then(|f| f.cover.as_mut()) {
                    c.apply = on;
                }
            }
        }
        Message::ToggleCoverAll(job, on) => {
            if let Some(j) = app.jobs.get_mut(job) {
                j.cover_all = on;
                j.cover_off.clear();
                j.rebuild(&app.cfg);
            }
        }
        Message::ToggleDelete(job, idx, on) => {
            if let Some(j) = app.jobs.get_mut(job) {
                if on {
                    j.delete_off.remove(&idx);
                } else {
                    j.delete_off.insert(idx);
                }
                if let Some(d) = j.plan.as_mut().and_then(|p| p.delete_files.get_mut(idx)) {
                    d.apply = on;
                }
            }
        }
        Message::ToggleLog => app.log_open = !app.log_open,
        Message::ClearLog => {
            let _ = tagjoe_core::log::clear();
            app.log.clear();
        }
        Message::ApplyProgress(job, p) => {
            let unit = app.jobs.get(job).map(|j| j.unit.path.display().to_string()).unwrap_or_default();
            let name = p.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if !p.finished {
                // announced before the write: show the name at once, nothing to log yet
                if let Some(j) = app.jobs.get_mut(job) {
                    j.progress = Some((p.done, p.total, name.clone()));
                }
                app.status = format!("{} {} {} {} · {name}", i18n::t("Writing"), p.done + 1, i18n::t("of"), p.total);
                return Task::none();
            }
            let action = if !p.ok {
                format!("{}: {}", i18n::t("error"), p.message)
            } else if p.message == "deleted" {
                i18n::t("deleted")
            } else {
                p.message.clone()
            };
            app.log(&unit, &name, &action, p.ok);
            if let Some(j) = app.jobs.get_mut(job) {
                j.progress = Some((p.done, p.total, name.clone()));
            }
            app.status = format!("{} {} {} {} · {name}", i18n::t("Writing"), p.done, i18n::t("of"), p.total);
        }
        Message::ToggleRename(file, on) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                if on {
                    j.rename_off.remove(&file);
                } else {
                    j.rename_off.insert(file);
                }
                j.rebuild(&app.cfg);
            }
        }
        Message::ToggleRenameAll(job, on) => {
            if let Some(j) = app.jobs.get_mut(job) {
                if on {
                    j.rename_off.clear();
                } else {
                    j.rename_off = (0..j.unit.files.len()).collect();
                }
                j.rebuild(&app.cfg);
            }
        }
        Message::PaneResized(e) => app.panes.resize(e.split, e.ratio.clamp(0.15, 0.85)),
        Message::OpenFolderPicker => {
            let start = app.selected.and_then(|i| app.jobs.get(i)).and_then(|j| j.unit.path.parent().map(|p| p.to_path_buf()));
            app.status = i18n::t("Choose a folder in the system dialog…");
            return work::pick_folder_task(start);
        }
        Message::FolderPicked(Some(dir)) => {
            app.status = format!("{} {}", i18n::t("Scanning"), dir.display());
            return work::scan_task(dir);
        }
        Message::FolderPicked(None) => app.status = i18n::t("No folder chosen."),
        Message::OpenCoverPicker(job) => {
            let start = app.jobs.get(job).map(|j| j.unit.path.clone());
            app.status = i18n::t("Choose a cover image in the system dialog…");
            return work::pick_image_task(job, start);
        }
        Message::CoverPicked(job, Some(file)) => {
            if let Some(j) = app.jobs.get_mut(job) {
                j.cover_override = Some(CoverSource::File { path: file });
                j.cover_off.clear();
                j.rebuild(&app.cfg);
                app.status = i18n::t("Cover set.");
            }
        }
        Message::CoverPicked(_, None) => app.status = i18n::t("No image chosen."),
        Message::OpenUrl(url) => open_url(&url),
        Message::EditNew(file, field, value) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                if field.is_album_level() {
                    j.album_overrides.insert(field, value);
                } else {
                    j.file_overrides.insert((file, field), value);
                }
                j.rebuild(&app.cfg);
            }
        }
        Message::EditAlbum(job, field, value) => {
            if let Some(j) = app.jobs.get_mut(job) {
                j.album_overrides.insert(field, value);
                j.rebuild(&app.cfg);
            }
        }
        Message::CoverInput(job, s) => {
            if let Some(j) = app.jobs.get_mut(job) {
                j.cover_input = s;
            }
        }
        Message::CoverUse(job) => {
            if let Some(j) = app.jobs.get_mut(job) {
                let s = j.cover_input.trim().to_string();
                if s.starts_with("http://") || s.starts_with("https://") {
                    return work::fetch_url_task(job, s, app.cfg.clone());
                }
                let p = PathBuf::from(&s);
                if p.is_file() {
                    j.cover_override = Some(CoverSource::File { path: p });
                    j.rebuild(&app.cfg);
                    j.cover_input.clear();
                } else {
                    app.status = i18n::t("Cover: enter an image file path or an http(s) URL.");
                }
            }
        }
        Message::CoverFetched(job, r) => {
            if let Some(j) = app.jobs.get_mut(job) {
                match r {
                    Ok(bytes) => {
                        j.cover_override = Some(CoverSource::Bytes { bytes, mime: "image/jpeg".into() });
                        j.cover_input.clear();
                        j.rebuild(&app.cfg);
                    }
                    Err(e) => app.status = format!("{}: {e}", i18n::t("Cover")),
                }
            }
        }
        Message::CoverFromFolder(job) => {
            if let Some(j) = app.jobs.get_mut(job) {
                if let Some(p) = j.unit.cover_files.first() {
                    j.cover_override = Some(CoverSource::File { path: p.clone() });
                    j.rebuild(&app.cfg);
                }
            }
        }
        Message::Apply(job) | Message::CleanOnly(job) => {
            let clean = matches!(msg, Message::CleanOnly(_));
            if let Some(j) = app.jobs.get_mut(job) {
                if let Some(mut plan) = j.plan.clone() {
                    if clean {
                        plan.new_folder = None;
                        for f in &mut plan.files {
                            f.new_name = None;
                            f.changes.clear();
                            f.cover = None;
                        }
                    }
                    j.state = JobState::Applying;
                    let total = plan.files.iter().filter(|f| f.apply && !f.is_noop()).count() + plan.deletions();
                    j.progress = Some((0, total, String::new()));
                    let unit = j.unit.path.display().to_string();
                    app.status = format!("{} 0 {} {total}", i18n::t("Writing"), i18n::t("of"));
                    app.log(&unit, "", &format!("{} · {total} {}", i18n::t("run started"), i18n::t("steps")), true);
                    return work::apply_task(job, plan, app.cfg.clone());
                }
            }
        }
        Message::Applied(job, result) => {
            app.last_journal = work::last_journal();
            let unit = app.jobs.get(job).map(|j| j.unit.path.display().to_string()).unwrap_or_default();
            if let Some((from, to)) = &result.folder {
                app.log(&unit, "", &format!("{} {} → {}", i18n::t("folder renamed"), from.display(), to.display()), true);
            }
            let written = result.files.iter().filter(|f| f.ok && f.message != "deleted").count();
            let deleted = result.deleted.len();
            let mut summary = format!("{} {written} {}", i18n::t("Written:"), i18n::t("file(s)"));
            if deleted > 0 {
                summary.push_str(&format!(", {deleted} {}", i18n::t("deleted")));
            }
            if result.errors > 0 {
                summary.push_str(&format!(", {} {}", result.errors, i18n::t("error(s)")));
            }
            app.log(&unit, "", &format!("{} · {summary}", i18n::t("run finished")), result.errors == 0);
            let remove = app.cfg.remove_done_from_queue && result.errors == 0;
            if let Some(j) = app.jobs.get_mut(job) {
                j.progress = None;
                j.state = if result.errors == 0 { JobState::Done(summary.clone()) } else { JobState::Failed(summary.clone()) };
                j.result = Some(result);
                app.status = summary;
            }
            if remove && job < app.jobs.len() {
                app.jobs.remove(job);
                app.selected = match app.selected {
                    Some(s) if s == job => if app.jobs.is_empty() { None } else { Some(s.min(app.jobs.len() - 1)) },
                    Some(s) if s > job => Some(s - 1),
                    other => other,
                };
            }
        }
        Message::Undo => {
            app.status = i18n::t("Undoing the last run…");
            return work::undo_task(app.cfg.clone());
        }
        Message::Undoned => {}
        Message::Undone(r) => {
            app.last_journal = work::last_journal();
            app.status = match r {
                Ok((unit, res)) => {
                    for f in &res.files {
                        let name = f.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        app.log(&unit, &name, &if f.ok { format!("{} · {}", i18n::t("undone"), f.message) } else { format!("{}: {}", i18n::t("error"), f.message) }, f.ok);
                    }
                    format!("{} {unit} ({} {})", i18n::t("Undone:"), res.files.len(), i18n::t("file(s)"))
                }
                Err(e) => {
                    app.log("", "", &format!("{}: {e}", i18n::t("Undo failed")), false);
                    format!("{}: {e}", i18n::t("Undo failed"))
                }
            };
        }
        Message::OpenSettings => {
            app.settings = Some(settings::SettingsState::new((*app.cfg).clone()));
        }
        Message::Settings(m) => {
            let mut close = false;
            let mut check = false;
            let mut fpcalc = false;
            let mut save: Option<Config> = None;
            if let Some(s) = app.settings.as_mut() {
                match s.update(m) {
                    settings::Outcome::Keep => {}
                    settings::Outcome::Close => close = true,
                    settings::Outcome::Save(cfg) => {
                        save = Some(*cfg);
                        close = true;
                    }
                    settings::Outcome::CheckUpdate(cfg) => {
                        save = Some(*cfg);
                        close = true;
                        check = true;
                    }
                    settings::Outcome::DownloadFpcalc(cfg) => {
                        save = Some(*cfg);
                        close = true;
                        fpcalc = true;
                    }
                }
            }
            if let Some(cfg) = save {
                match cfg.save() {
                    Ok(()) => app.status = i18n::t("Settings saved."),
                    Err(e) => app.status = format!("{}: {e}", i18n::t("Cannot save settings")),
                }
                app.cfg = Arc::new(cfg);
                for j in app.jobs.iter_mut() {
                    if j.state == JobState::Ready {
                        j.rebuild(&app.cfg);
                    }
                }
            }
            if close {
                app.settings = None;
            }
            if check {
                return update(app, Message::CheckUpdate(true));
            }
            if fpcalc {
                app.status = i18n::t("Downloading fpcalc (Chromaprint) and checking its SHA-256 sum…");
                return work::fpcalc_download_task();
            }
        }
        Message::FpcalcDownloaded(r) => {
            app.status = match r {
                Ok(p) => {
                    app.log("", "fpcalc", &format!("{} {}", i18n::t("downloaded to"), p.display()), true);
                    format!("{} {}", i18n::t("fpcalc ready:"), p.display())
                }
                Err(e) => {
                    app.log("", "fpcalc", &format!("{}: {e}", i18n::t("error")), false);
                    format!("{}: {e}", i18n::t("fpcalc download failed"))
                }
            };
        }
        Message::CheckUpdate(manual) => {
            app.update = Some(UpdateState::Checking);
            app.status = i18n::t("Asking GitHub for the latest release…");
            return work::update_check_task(manual);
        }
        Message::UpdateChecked(manual, r) => {
            let mut cfg = (*app.cfg).clone();
            cfg.update_last_check = tagjoe_core::update::now();
            let _ = cfg.save();
            app.cfg = Arc::new(cfg);
            app.update = match r {
                Ok(Some(rel)) => {
                    app.status = format!("{} {}", i18n::t("Update available:"), rel.tag_name);
                    Some(UpdateState::Offered(rel))
                }
                Ok(None) => {
                    app.status = format!("{} {}", i18n::t("No newer version. Installed:"), tagjoe_core::VERSION);
                    if manual { Some(UpdateState::UpToDate) } else { None }
                }
                Err(e) => {
                    app.status = format!("{}: {e}", i18n::t("Update check failed"));
                    if manual { Some(UpdateState::Failed(e)) } else { None }
                }
            };
        }
        Message::UpdateDownload => {
            if let Some(UpdateState::Offered(rel)) = app.update.clone() {
                app.update = Some(UpdateState::Downloading(rel.clone()));
                app.status = i18n::t("Downloading the package and checking its SHA-256 sum…");
                return work::update_download_task(rel);
            }
        }
        Message::UpdateDownloaded(r) => {
            if let Some(UpdateState::Downloading(rel)) = app.update.clone() {
                app.update = Some(match r {
                    Ok(p) => {
                        app.status = i18n::t("Package verified.");
                        UpdateState::Ready(rel, p)
                    }
                    Err(e) => {
                        app.status = format!("{}: {e}", i18n::t("Download failed"));
                        UpdateState::Failed(e)
                    }
                });
            }
        }
        Message::UpdateInstall => {
            if let Some(UpdateState::Ready(_, p)) = app.update.clone() {
                let cmd = tagjoe_core::update::install_command(&p);
                match std::process::Command::new(&cmd[0]).args(&cmd[1..]).spawn() {
                    Ok(_) => {
                        app.status = i18n::t("Installer started – the system asks for your password. Restart tagjoe afterwards.");
                        app.update = None;
                    }
                    Err(e) => app.status = format!("{}: {e}", i18n::t("Cannot start the installer")),
                }
            }
        }
        Message::UpdateDismiss => app.update = None,
        Message::Status(s) => app.status = s,
        Message::ToggleOffline(b) => app.offline = b,
    }
    Task::none()
}

/// Open a web address in the system browser (no extra dependency).
pub fn open_url(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    #[cfg(target_os = "linux")]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    let _ = r;
}

fn view(app: &App) -> Element<'_, Message> {
    if let Some(s) = &app.settings {
        return settings::view(s, app).map(Message::Settings);
    }
    view::main(app)
}

/// Linux: the application id lets Wayland/X11 match the window to the .desktop file (icon, dock).
/// Other systems have no such field in iced's `PlatformSpecific`.
#[cfg(target_os = "linux")]
fn platform_settings() -> window::settings::PlatformSpecific {
    window::settings::PlatformSpecific { application_id: "io.github.joeMJ.tagjoe".into(), ..Default::default() }
}
#[cfg(not(target_os = "linux"))]
fn platform_settings() -> window::settings::PlatformSpecific {
    Default::default()
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title(|_: &App| i18n::t("tagjoe"))
        .theme(|app: &App| Some(app.theme.clone()))
        .subscription(subscription)
        .window(window::Settings {
            size: iced::Size::new(1280.0, 860.0),
            min_size: Some(iced::Size::new(900.0, 600.0)),
            // X11/Windows: window icon; Wayland: the application id matches the .desktop file,
            // so the desktop shows the installed icon.
            icon: window::icon::from_file_data(include_bytes!("../../../data/icons/io.github.joeMJ.tagjoe-256.png"), None).ok(),
            platform_specific: platform_settings(),
            ..Default::default()
        })
        .run()
}
