//! Built-in file/folder picker (no native dialog dependency): navigate directories, choose a
//! folder or an image file.

use crate::i18n::t;
use crate::theme as th;
use iced::widget::{button, column, container, mouse_area, opaque, row, scrollable, space, stack, text, text_input};
use iced::{Element, Fill, Length};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMode {
    Folder,
    Image,
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub mode: PickMode,
    /// Job the cover is for (Image mode).
    pub job: Option<usize>,
    pub dir: PathBuf,
    pub path_input: String,
    pub dirs: Vec<String>,
    pub files: Vec<String>,
    pub audio_count: usize,
    pub error: String,
}

#[derive(Debug, Clone)]
pub enum PickerMessage {
    Navigate(PathBuf),
    Up,
    Home,
    PathInput(String),
    PathSubmit,
    ChooseFile(PathBuf),
    ChooseFolder,
    Cancel,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

impl Picker {
    pub fn new(mode: PickMode, job: Option<usize>, start: Option<&Path>) -> Self {
        let dir = start.filter(|p| p.is_dir()).map(Path::to_path_buf).unwrap_or_else(home);
        let mut p = Picker { mode, job, dir: dir.clone(), path_input: dir.display().to_string(), dirs: vec![], files: vec![], audio_count: 0, error: String::new() };
        p.read();
        p
    }

    fn read(&mut self) {
        self.dirs.clear();
        self.files.clear();
        self.audio_count = 0;
        self.error.clear();
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    let p = e.path();
                    if p.is_dir() {
                        self.dirs.push(name);
                    } else if tagjoe_core::scan::is_audio(&p) {
                        self.audio_count += 1;
                    } else if self.mode == PickMode::Image && tagjoe_core::scan::is_image(&p) {
                        self.files.push(name);
                    }
                }
                self.dirs.sort_by_key(|a| a.to_lowercase());
                self.files.sort_by_key(|a| a.to_lowercase());
            }
            Err(e) => self.error = format!("{}: {e}", self.dir.display()),
        }
        self.path_input = self.dir.display().to_string();
    }

    /// Returns Some(chosen path) when the picker finished, None while it stays open or was cancelled.
    pub fn update(&mut self, m: PickerMessage) -> Option<Option<PathBuf>> {
        match m {
            PickerMessage::Navigate(p) => {
                self.dir = p;
                self.read();
            }
            PickerMessage::Up => {
                if let Some(p) = self.dir.parent() {
                    self.dir = p.to_path_buf();
                    self.read();
                }
            }
            PickerMessage::Home => {
                self.dir = home();
                self.read();
            }
            PickerMessage::PathInput(s) => self.path_input = s,
            PickerMessage::PathSubmit => {
                let p = PathBuf::from(self.path_input.trim());
                if p.is_dir() {
                    self.dir = p;
                    self.read();
                } else if self.mode == PickMode::Image && p.is_file() {
                    return Some(Some(p));
                } else {
                    self.error = t("Not a folder.");
                }
            }
            PickerMessage::ChooseFile(p) => return Some(Some(p)),
            PickerMessage::ChooseFolder => return Some(Some(self.dir.clone())),
            PickerMessage::Cancel => return Some(None),
        }
        None
    }

    pub fn view(&self) -> Element<'_, PickerMessage> {
        let title = match self.mode {
            PickMode::Folder => t("Choose a folder"),
            PickMode::Image => t("Choose a cover image"),
        };
        let path = row![
            button(text(t("Up")).size(12)).on_press(PickerMessage::Up).style(button::secondary).padding([5, 8]),
            button(text(t("Home")).size(12)).on_press(PickerMessage::Home).style(button::secondary).padding([5, 8]),
            text_input("", &self.path_input).on_input(PickerMessage::PathInput).on_submit(PickerMessage::PathSubmit).size(13).padding(5).width(Fill),
        ]
        .spacing(6)
        .align_y(iced::Center);
        let mut list = column![].spacing(2);
        for d in &self.dirs {
            let p = self.dir.join(d);
            list = list.push(button(text(format!("▸ {d}")).size(13)).on_press(PickerMessage::Navigate(p)).style(th::list_button(false)).width(Fill).padding([4, 8]));
        }
        for f in &self.files {
            let p = self.dir.join(f);
            list = list.push(button(text(format!("🖼 {f}")).size(13)).on_press(PickerMessage::ChooseFile(p)).style(th::list_button(false)).width(Fill).padding([4, 8]));
        }
        let info = if self.audio_count > 0 { format!("{} {}", self.audio_count, t("audio files")) } else { String::new() };
        let mut footer = row![text(info).size(12).style(th::dim).width(Fill), button(text(t("Cancel")).size(13)).on_press(PickerMessage::Cancel).style(button::secondary).padding([6, 10])].spacing(8).align_y(iced::Center);
        if self.mode == PickMode::Folder {
            footer = footer.push(button(text(t("Use this folder")).size(13)).on_press(PickerMessage::ChooseFolder).style(button::primary).padding([6, 12]));
        }
        let err: Element<'_, PickerMessage> = if self.error.is_empty() { text("").into() } else { text(self.error.clone()).size(12).style(th::err).into() };
        let body = column![text(title).size(15), path, err, scrollable(list).height(Fill), footer].spacing(8);
        container(body).padding(14).width(Length::Fixed(720.0)).height(Length::Fixed(520.0)).style(th::card).into()
    }
}

/// Dim the page and show the picker on top.
pub fn overlay<'a, M: Clone + 'a>(base: Element<'a, M>, dialog: Element<'a, M>, on_backdrop: M) -> Element<'a, M> {
    let backdrop = mouse_area(container(space()).width(Fill).height(Fill).style(|_t: &iced::Theme| container::Style { background: Some(iced::Color { a: 0.45, ..iced::Color::BLACK }.into()), ..Default::default() })).on_press(on_backdrop);
    stack![base, opaque(backdrop), opaque(container(dialog).center(Fill))].into()
}
