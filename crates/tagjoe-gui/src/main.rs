//! tagjoe desktop application (iced).
#![forbid(unsafe_code)]

mod i18n;
mod settings;
mod theme;
mod view;
mod work;

use iced::widget::image;
use iced::{event, window, Element, Subscription, Task, Theme};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tagjoe_core::apply::ApplyResult;
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
    Applied(usize, ApplyResult),
    Undo,
    Undoned,
    Undone(Result<(String, ApplyResult), String>),
    OpenSettings,
    Settings(settings::SettingsMessage),
    Status(String),
    ToggleOffline(bool),
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
    let app = App { cfg: Arc::new(cfg), theme: if a.theme.as_deref() == Some("dark") { theme::dark() } else { theme::light() }, jobs: vec![], selected: None, path_input: String::new(), status: i18n::t("Drop a folder onto the window or enter a path."), settings: None, offline: a.offline, last_journal: work::last_journal() };
    let mut tasks = Vec::new();
    if a.theme.is_none() {
        tasks.push(iced::system::theme().map(Message::SystemTheme));
    }
    for f in a.folders {
        tasks.push(work::scan_task(f));
    }
    (app, Task::batch(tasks))
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
        Job { unit, state: JobState::Scanning, candidates: vec![], chosen: None, plan: None, selected_file: 0, album_overrides: BTreeMap::new(), file_overrides: BTreeMap::new(), cover_override: None, cover_handle: None, notes: vec![], cover_input: String::new(), result: None }
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
        // manual values always apply, even when they equal nothing or differ from the source
        for fp in plan.files.iter_mut() {
            for ch in fp.changes.iter_mut() {
                let manual = self.album_overrides.contains_key(&ch.field) || self.file_overrides.contains_key(&(0, ch.field));
                if manual && ch.status != Status::Ok {
                    ch.apply = true;
                }
            }
        }
        self.plan = Some(plan);
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
            if let Some(p) = app.selected.and_then(|s| app.jobs.get_mut(s)).and_then(|j| j.plan.as_mut()) {
                if let Some(c) = p.files.get_mut(file).and_then(|f| f.changes.iter_mut().find(|c| c.field == field)) {
                    c.apply = on;
                }
            }
        }
        Message::ToggleRemoval(file, idx, on) => {
            if let Some(p) = app.selected.and_then(|s| app.jobs.get_mut(s)).and_then(|j| j.plan.as_mut()) {
                if let Some(r) = p.files.get_mut(file).and_then(|f| f.removals.get_mut(idx)) {
                    r.apply = on;
                }
            }
        }
        Message::ToggleCover(file, on) => {
            if let Some(p) = app.selected.and_then(|s| app.jobs.get_mut(s)).and_then(|j| j.plan.as_mut()) {
                if let Some(c) = p.files.get_mut(file).and_then(|f| f.cover.as_mut()) {
                    c.apply = on;
                }
            }
        }
        Message::ToggleRename(file, on) => {
            if let Some(j) = app.selected.and_then(|s| app.jobs.get_mut(s)) {
                if let Some(p) = j.plan.as_mut() {
                    if let Some(f) = p.files.get_mut(file) {
                        if !on {
                            f.new_name = None;
                        } else if f.new_name.is_none() {
                            // recompute by rebuilding the whole plan
                            j.rebuild(&app.cfg);
                        }
                    }
                }
            }
        }
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
                    return work::apply_task(job, plan, app.cfg.clone());
                }
            }
        }
        Message::Applied(job, result) => {
            app.last_journal = work::last_journal();
            if let Some(j) = app.jobs.get_mut(job) {
                let ok = result.files.iter().filter(|f| f.ok).count();
                let msg = if result.errors == 0 { format!("{} {ok} {}", i18n::t("Written:"), i18n::t("file(s)")) } else { format!("{} {}", result.errors, i18n::t("error(s)")) };
                j.state = if result.errors == 0 { JobState::Done(msg.clone()) } else { JobState::Failed(msg.clone()) };
                j.result = Some(result);
                app.status = msg;
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
                Ok((unit, res)) => format!("{} {unit} ({} {})", i18n::t("Undone:"), res.files.len(), i18n::t("file(s)")),
                Err(e) => format!("{}: {e}", i18n::t("Undo failed")),
            };
        }
        Message::OpenSettings => {
            app.settings = Some(settings::SettingsState::new((*app.cfg).clone()));
        }
        Message::Settings(m) => {
            let mut close = false;
            let mut save: Option<Config> = None;
            if let Some(s) = app.settings.as_mut() {
                match s.update(m) {
                    settings::Outcome::Keep => {}
                    settings::Outcome::Close => close = true,
                    settings::Outcome::Save(cfg) => {
                        save = Some(cfg);
                        close = true;
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
        }
        Message::Status(s) => app.status = s,
        Message::ToggleOffline(b) => app.offline = b,
    }
    Task::none()
}

fn view(app: &App) -> Element<'_, Message> {
    if let Some(s) = &app.settings {
        return settings::view(s, app).map(Message::Settings);
    }
    view::main(app)
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title(|_: &App| i18n::t("tagjoe"))
        .theme(|app: &App| Some(app.theme.clone()))
        .subscription(subscription)
        .window(window::Settings { size: iced::Size::new(1280.0, 860.0), min_size: Some(iced::Size::new(900.0, 600.0)), ..Default::default() })
        .run()
}
