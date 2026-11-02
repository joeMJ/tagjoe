//! Main window: header, queue, release card, files table, fields table, footer.

use crate::i18n::t;
use crate::theme as th;
use crate::{App, Job, JobState, Message};
use iced::widget::{button, checkbox, column, container, image, row, rule, scrollable, space, text, text_input};
use iced::{Element, Fill, FillPortion, Length, Theme};
use tagjoe_core::model::{Field, Kind};
use tagjoe_core::schema::Status;

fn status_text<'a>(status: Status, label: String) -> Element<'a, Message> {
    let style: fn(&Theme) -> text::Style = match status {
        Status::Ok => th::ok,
        Status::Missing => th::err,
        Status::Differs => th::warn,
        Status::Foreign => th::err,
    };
    text(format!("● {label}")).style(style).size(13).into()
}

fn kind_badge<'a>(kind: Kind) -> Element<'a, Message> {
    let label = match kind {
        Kind::Album => t("Album"),
        Kind::Compilation => t("Compilation"),
        Kind::Audiobook => t("Audiobook"),
        Kind::Unknown => t("Unknown"),
    };
    container(text(label).size(12).style(if kind == Kind::Unknown { th::dim } else { th::info })).padding([1, 7]).style(|theme: &Theme| {
        let c = th::colors(theme);
        container::Style { border: iced::Border { color: c.card_border, width: 1.0, radius: 10.0.into() }, ..Default::default() }
    }).into()
}

fn queue_entry<'a>(i: usize, job: &'a Job, selected: bool) -> Element<'a, Message> {
    let (complete, incomplete, foreign, no_cover, _renames) = job.summary();
    let n = job.unit.files.len();
    let state_line: Element<'a, Message> = match &job.state {
        JobState::Scanning => text(format!("● {}", t("reading…"))).size(12).style(th::info).into(),
        JobState::Identifying(s) => text(format!("● {} {s}", t("identifying"))).size(12).style(th::info).into(),
        JobState::Ready => {
            if incomplete == 0 && foreign == 0 && no_cover == 0 {
                text(format!("● {}", t("complete"))).size(12).style(th::ok).into()
            } else if job.chosen.is_none() && !job.candidates.is_empty() || job.chosen.is_none() && job.unit.kind == Kind::Audiobook {
                text(format!("● {}", t("no match – complete manually"))).size(12).style(th::err).into()
            } else {
                text(format!("● {incomplete} {}{}", t("incomplete"), if no_cover > 0 { format!(" · {}", t("cover missing")) } else { String::new() })).size(12).style(th::warn).into()
            }
        }
        JobState::Applying => text(format!("● {}", t("writing…"))).size(12).style(th::info).into(),
        JobState::Done(m) => text(format!("● {m}")).size(12).style(th::ok).into(),
        JobState::Failed(m) => text(format!("● {m}")).size(12).style(th::err).into(),
    };
    let _ = complete;
    let body = column![text(job.unit.name()).size(14), row![kind_badge(job.unit.kind), text(format!("{n} {}", t("files"))).size(12).style(th::dim)].spacing(6).align_y(iced::Center), state_line].spacing(3);
    button(body).on_press(Message::SelectJob(i)).style(th::list_button(selected)).width(Fill).padding(8).into()
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(6);
    for (i, j) in app.jobs.iter().enumerate() {
        list = list.push(queue_entry(i, j, app.selected == Some(i)));
    }
    let drop = container(column![text(t("Drop a folder here")).size(14), text(t("or enter a path above")).size(12)].spacing(2).align_x(iced::Center)).padding(16).width(Fill).style(th::drop_zone);
    let input = row![text_input(&t("Folder path…"), &app.path_input).on_input(Message::PathInput).on_submit(Message::AddPath).size(13).padding(6), button(text(t("Add")).size(13)).on_press(Message::AddPath).padding([6, 10]).style(button::secondary)].spacing(6);
    container(column![text(t("QUEUE")).size(12).style(th::dim), input, scrollable(list).height(Fill), drop].spacing(10).padding(10)).width(300).height(Fill).style(th::sidebar).into()
}

fn header(app: &App) -> Element<'_, Message> {
    let offline = checkbox(app.offline).label(t("Offline")).on_toggle(Message::ToggleOffline).size(16).text_size(13);
    container(row![text(t("tagjoe")).size(16), space().width(Fill), offline, button(text(t("Undo last run")).size(13)).on_press_maybe(app.last_journal.as_ref().map(|_| Message::Undo)).style(button::secondary).padding([6, 10]), button(text(t("Settings")).size(13)).on_press(Message::OpenSettings).style(button::secondary).padding([6, 10])].spacing(10).align_y(iced::Center).padding([6, 12])).width(Fill).style(th::header_bar).into()
}

#[derive(Clone)]
struct FileRow {
    idx: usize,
    old: String,
    new: Option<String>,
    tags: (Status, String),
    cover: Option<(bool, String)>,
    selected: bool,
}

#[derive(Clone)]
struct FieldRow {
    file: usize,
    field: Field,
    label: String,
    old: String,
    new: String,
    status: Status,
    apply: bool,
}

#[derive(Clone)]
struct RemovalRow {
    file: usize,
    idx: usize,
    key: String,
    value: String,
    apply: bool,
}

fn release_card<'a>(job_idx: usize, job: &'a Job) -> Element<'a, Message> {
    let cover: Element<'a, Message> = match &job.cover_handle {
        Some(h) => container(image(h.clone()).width(96).height(96).content_fit(iced::ContentFit::Contain)).width(96).height(96).into(),
        None => container(text(t("no cover")).size(11).style(th::dim)).width(96).height(96).center(iced::Length::Fill).style(th::drop_zone).into(),
    };
    let (title, line1, line2) = match job.chosen.and_then(|i| job.candidates.get(i)) {
        Some(c) => {
            let a = &c.release.album;
            let title = a.get(&Field::Album).cloned().unwrap_or_default();
            let line1 = [a.get(&Field::AlbumArtist).cloned().unwrap_or_default(), a.get(&Field::Date).map(|d| d.chars().take(4).collect()).unwrap_or_default(), a.get(&Field::Label).or(a.get(&Field::Publisher)).cloned().unwrap_or_default(), a.get(&Field::CatalogNumber).cloned().unwrap_or_default(), format!("{} {}", c.release.tracks.len(), t("tracks"))].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
            let line2 = format!("{} {} · {} {} % · {} {}, {} {}", t("Source"), c.release.source, t("match"), c.release.score, c.by_fingerprint, t("by fingerprint"), c.by_heuristic, t("by duration and title"));
            (title, line1, line2)
        }
        None => {
            let (rel, _) = job.reference(&tagjoe_core::Config::default());
            (rel.album.get(&Field::Album).cloned().unwrap_or_default(), rel.summary.clone(), if job.candidates.is_empty() { t("No source match – values from tags and folder names; complete the fields manually.") } else { t("Candidates below the threshold – pick one or complete manually.") })
        }
    };
    let mut cands = row![].spacing(6);
    for (i, c) in job.candidates.iter().enumerate() {
        let sel = job.chosen == Some(i);
        cands = cands.push(button(text(format!("{} % · {}", c.release.score, c.release.summary)).size(12)).on_press(Message::ChooseCandidate(job_idx, i)).style(th::list_button(sel)).padding([4, 8]));
    }
    cands = cands.push(button(text(t("tags only")).size(12)).on_press(Message::ChooseCandidate(job_idx, usize::MAX)).style(th::list_button(job.chosen.is_none())).padding([4, 8]));
    let cands = cands.wrap();
    let (complete, incomplete, foreign, no_cover, renames) = job.summary();
    let chips = row![
        chip(format!("{complete} {}", t("complete")), th::ok),
        chip(format!("{incomplete} {}", t("incomplete")), th::warn),
        chip(format!("{foreign} {}", t("foreign fields")), th::err),
        chip(format!("{} {no_cover}", t("cover missing:")), th::err),
        chip(format!("{renames} {}", t("renames")), th::dim),
    ]
    .spacing(6);
    let cover_tools = row![
        text_input(&t("Cover: image file path or URL"), &job.cover_input).on_input(move |s| Message::CoverInput(job_idx, s)).on_submit(Message::CoverUse(job_idx)).size(12).padding(5).width(Length::Fixed(320.0)),
        button(text(t("Use")).size(12)).on_press(Message::CoverUse(job_idx)).padding([5, 8]).style(button::secondary),
        button(text(t("From folder")).size(12)).on_press_maybe(job.unit.cover_files.first().map(|_| Message::CoverFromFolder(job_idx))).padding([5, 8]).style(button::secondary),
    ]
    .spacing(6);
    let info = column![text(title).size(16), text(line1).size(13).style(th::dim), text(line2).size(12).style(th::dim), cands, cover_tools].spacing(4);
    let mut notes = column![].spacing(2);
    for n in &job.notes {
        notes = notes.push(text(n.clone()).size(11).style(th::dim));
    }
    container(column![row![cover, info.width(Fill)].spacing(12).align_y(iced::Top), chips, notes].spacing(8)).padding(10).width(Fill).style(th::card).into()
}

fn chip<'a>(label: String, style: fn(&Theme) -> text::Style) -> Element<'a, Message> {
    container(text(label).size(12).style(style)).padding([3, 8]).style(|theme: &Theme| {
        let c = th::colors(theme);
        container::Style { background: Some(c.view.into()), border: iced::Border { color: c.card_border, width: 1.0, radius: 6.0.into() }, ..Default::default() }
    }).into()
}

fn files_table<'a>(job: &'a Job) -> Element<'a, Message> {
    let Some(plan) = &job.plan else { return text(t("no plan")).into() };
    let rows: Vec<FileRow> = plan
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let missing = f.changes.iter().filter(|c| c.status == Status::Missing && c.apply).count();
            let differs = f.changes.iter().filter(|c| c.status == Status::Differs && c.apply).count();
            let still_missing = f.changes.iter().filter(|c| c.status == Status::Missing && c.new.is_empty()).count();
            let foreign = f.removals.iter().filter(|r| r.apply).count();
            let tags = if missing + differs + foreign + still_missing == 0 {
                (Status::Ok, t("complete"))
            } else {
                let mut parts = Vec::new();
                if missing > 0 {
                    parts.push(format!("{missing} {}", t("to add")));
                }
                if differs > 0 {
                    parts.push(format!("{differs} {}", t("differ")));
                }
                if foreign > 0 {
                    parts.push(format!("{foreign} {}", t("foreign")));
                }
                if still_missing > 0 {
                    parts.push(format!("{still_missing} {}", t("unknown")));
                }
                (if still_missing > 0 { Status::Missing } else { Status::Differs }, parts.join(", "))
            };
            let has_cover = job.unit.files.get(i).map(|x| x.has_cover()).unwrap_or(false);
            let cover = match &f.cover {
                Some(c) if c.apply => Some((true, format!("{} {}×{}", if c.replaces_existing { t("replace") } else { t("add") }, c.width, c.height))),
                _ if has_cover => Some((false, t("present"))),
                _ => Some((false, t("missing"))),
            };
            FileRow { idx: i, old: f.file_name.clone(), new: f.new_name.clone(), tags, cover, selected: job.selected_file == i }
        })
        .collect();
    let head = row![
        text(t("No")).size(12).style(th::dim).width(Length::Fixed(36.0)),
        text(t("File (current → proposed)")).size(12).style(th::dim).width(Fill),
        text(t("Tags")).size(12).style(th::dim).width(Length::Fixed(230.0)),
        text(t("Cover")).size(12).style(th::dim).width(Length::Fixed(150.0)),
    ]
    .spacing(8);
    let mut body = column![].spacing(2);
    for r in rows {
        let idx = r.idx;
        let name_col = column![text(r.old).size(12).font(iced::Font::MONOSPACE).style(th::dim), match r.new {
            Some(n) => text(n).size(12).font(iced::Font::MONOSPACE),
            None => text(t("(name unchanged)")).size(11).style(th::dim),
        }]
        .spacing(2)
        .width(Fill);
        let cover_cell: Element<'a, Message> = match r.cover {
            Some((true, s)) => status_text(Status::Differs, s),
            Some((false, s)) if s == t("present") => status_text(Status::Ok, s),
            Some((false, s)) => status_text(Status::Missing, s),
            None => text("").into(),
        };
        let line = row![
            text(format!("{:02}", idx + 1)).size(12).width(Length::Fixed(36.0)),
            name_col,
            container(status_text(r.tags.0, r.tags.1)).width(Length::Fixed(230.0)),
            container(cover_cell).width(Length::Fixed(150.0)),
        ]
        .spacing(8)
        .width(Fill)
        .align_y(iced::Top);
        body = body.push(button(line).on_press(Message::SelectFile(idx)).style(th::list_button(r.selected)).padding(6).width(Fill));
    }
    let tbl = column![head, rule::horizontal(1), body].spacing(4);
    container(column![text(t("FILES")).size(12).style(th::dim), scrollable(tbl).height(Fill)].spacing(6)).padding(10).width(Fill).height(Fill).style(th::card).into()
}

fn fields_table<'a>(job: &'a Job) -> Element<'a, Message> {
    let Some(plan) = &job.plan else { return text("").into() };
    let fi = job.selected_file.min(plan.files.len().saturating_sub(1));
    let Some(fp) = plan.files.get(fi) else { return text(t("no files")).into() };
    let kind = job.unit.kind;
    let rows: Vec<FieldRow> = fp.changes.iter().map(|c| FieldRow { file: fi, field: c.field, label: c.field.label(kind).to_string(), old: c.old.clone(), new: c.new.clone(), status: c.status, apply: c.apply }).collect();
    let removals: Vec<RemovalRow> = fp.removals.iter().enumerate().map(|(i, r)| RemovalRow { file: fi, idx: i, key: r.key.clone(), value: r.value.clone(), apply: r.apply }).collect();
    let head = row![
        text("").width(Length::Fixed(28.0)),
        text(t("Field")).size(12).style(th::dim).width(Length::Fixed(170.0)),
        text(t("Current")).size(12).style(th::dim).width(FillPortion(2)),
        text(t("New (editable)")).size(12).style(th::dim).width(FillPortion(2)),
        text(t("Status")).size(12).style(th::dim).width(Length::Fixed(120.0)),
    ]
    .spacing(8);
    let mut body = column![].spacing(2);
    for r in rows {
        let (file, field) = (r.file, r.field);
        let label = match r.status {
            Status::Ok => t("ok"),
            Status::Missing => {
                if r.new.is_empty() { t("missing") } else { t("missing → added") }
            }
            Status::Differs => t("differs"),
            Status::Foreign => t("foreign"),
        };
        let cb = checkbox(r.apply).on_toggle_maybe(if r.status == Status::Ok { None } else { Some(move |b| Message::ToggleChange(file, field, b)) }).size(16);
        let line = row![
            container(cb).width(Length::Fixed(28.0)),
            text(r.label).size(12).width(Length::Fixed(170.0)),
            text(if r.old.is_empty() { "—".to_string() } else { r.old }).size(12).style(th::dim).width(FillPortion(2)),
            text_input("", &r.new).on_input(move |s| Message::EditNew(file, field, s)).size(12).padding(4).width(FillPortion(2)),
            container(status_text(r.status, label)).width(Length::Fixed(120.0)),
        ]
        .spacing(8)
        .width(Fill)
        .align_y(iced::Center);
        body = body.push(line);
        body = body.push(rule::horizontal(1));
    }
    let tbl = column![head, rule::horizontal(1), body].spacing(4);
    let mut rem_body = column![].spacing(2);
    for r in removals {
        let (file, idx) = (r.file, r.idx);
        let line = row![
            container(checkbox(r.apply).on_toggle(move |b| Message::ToggleRemoval(file, idx, b)).size(16)).width(Length::Fixed(28.0)),
            text(r.key).size(12).font(iced::Font::MONOSPACE).width(Length::Fixed(220.0)),
            text(r.value.chars().take(80).collect::<String>()).size(12).style(th::dim).width(Fill),
            container(status_text(Status::Foreign, t("remove"))).width(Length::Fixed(120.0)),
        ]
        .spacing(8)
        .width(Fill)
        .align_y(iced::Center);
        rem_body = rem_body.push(line);
    }
    let rem_tbl: Element<'a, Message> = if fp.removals.is_empty() { text("").into() } else { column![text(t("FOREIGN ENTRIES")).size(12).style(th::dim), rule::horizontal(1), rem_body].spacing(4).into() };
    let mut extra = column![].spacing(4);
    if !fp.remove_containers.is_empty() {
        extra = extra.push(text(format!("{}: {}", t("Secondary containers to remove"), fp.remove_containers.join(", "))).size(12).style(th::warn));
    }
    if let Some(c) = &fp.cover {
        let (file, apply) = (fi, c.apply);
        extra = extra.push(checkbox(apply).label(format!("{} {}×{} ({} KB){}", t("Embed cover"), c.width, c.height, c.bytes / 1024, if c.replaces_existing { t(", replaces the existing one") } else { String::new() })).on_toggle(move |b| Message::ToggleCover(file, b)).size(16).text_size(12));
    }
    if fp.new_name.is_some() || job.plan.as_ref().map(|p| p.files[fi].new_name.is_some()).unwrap_or(false) {
        let file = fi;
        extra = extra.push(checkbox(true).label(format!("{} {}", t("Rename to"), fp.new_name.clone().unwrap_or_default())).on_toggle(move |b| Message::ToggleRename(file, b)).size(16).text_size(12));
    }
    let hint = text(t("Edits in “New” apply to this file; album-level fields apply to all files of the folder.")).size(11).style(th::dim);
    container(column![text(format!("{} · {} {}", t("FIELDS"), t("file"), fi + 1)).size(12).style(th::dim), hint, scrollable(column![tbl, rem_tbl, extra].spacing(8)).height(Fill)].spacing(6)).padding(10).width(Fill).height(Fill).style(th::card).into()
}

fn footer<'a>(app: &'a App, job_idx: Option<usize>, job: Option<&'a Job>) -> Element<'a, Message> {
    let ready = job.map(|j| j.state == JobState::Ready && j.plan.as_ref().map(|p| !p.is_noop()).unwrap_or(false)).unwrap_or(false);
    let n = job.and_then(|j| j.plan.as_ref()).map(|p| p.files.iter().filter(|f| !f.is_noop()).count()).unwrap_or(0);
    let journal = app.last_journal.clone().map(|j| format!("{} {j}", t("Journal:"))).unwrap_or_default();
    container(
        row![
            text(app.status.clone()).size(12).style(th::dim).width(Fill),
            text(journal).size(11).style(th::dim),
            button(text(t("Remove from queue")).size(13)).on_press_maybe(job_idx.filter(|_| job.map(|j| j.state != JobState::Applying).unwrap_or(false)).map(Message::RemoveJob)).style(button::secondary).padding([6, 10]),
            button(text(t("Clean only")).size(13)).on_press_maybe(job_idx.filter(|_| ready).map(Message::CleanOnly)).style(button::secondary).padding([6, 10]),
            button(text(format!("{} · {n} {}", t("Apply"), t("files"))).size(13)).on_press_maybe(job_idx.filter(|_| ready).map(Message::Apply)).style(button::primary).padding([6, 12]),
        ]
        .spacing(8)
        .align_y(iced::Center)
        .padding([6, 12]),
    )
    .width(Fill)
    .style(th::header_bar)
    .into()
}

fn kind_picker<'a>(job_idx: usize, job: &'a Job) -> Element<'a, Message> {
    let mut r = row![text(t("Type:")).size(12).style(th::dim)].spacing(4).align_y(iced::Center);
    for k in [Kind::Album, Kind::Compilation, Kind::Audiobook] {
        let label = match k {
            Kind::Album => t("Album"),
            Kind::Compilation => t("Compilation"),
            _ => t("Audiobook"),
        };
        r = r.push(button(text(label).size(12)).on_press(Message::SetKind(job_idx, k)).style(th::list_button(job.unit.kind == k)).padding([3, 8]));
    }
    r.into()
}

pub fn main(app: &App) -> Element<'_, Message> {
    let job = app.selected.and_then(|i| app.jobs.get(i).map(|j| (i, j)));
    let main: Element<'_, Message> = match job {
        Some((i, j)) => {
            let bar = row![text(j.unit.path.display().to_string()).size(14).width(Fill), kind_picker(i, j)].spacing(8).align_y(iced::Center);
            let split = column![container(files_table(j)).height(FillPortion(1)), container(fields_table(j)).height(FillPortion(1))].spacing(10).height(Fill);
            column![bar, release_card(i, j), split].spacing(10).padding(10).into()
        }
        None => container(column![text(t("Drop a folder (album, compilation or audiobook) onto the window.")).size(16), text(t("tagjoe reads every file, checks each tag field, proposes tags, cover and file names – nothing is written until you click Apply.")).size(13).style(th::dim)].spacing(8).align_x(iced::Center)).center(Fill).into(),
    };
    let body = row![sidebar(app), container(main).width(Fill).height(Fill)].height(Fill);
    column![header(app), body, footer(app, job.map(|(i, _)| i), job.map(|(_, j)| j))].into()
}
