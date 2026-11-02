//! Settings: templates (with live preview), sources, clean-up, tags, audiobook, general.

use crate::i18n::t;
use crate::theme as th;
use crate::App;
use iced::widget::{button, column, container, radio, row, scrollable, space, text, text_input, toggler};
use iced::{Element, Fill, Length};
use tagjoe_core::config::{Config, DiscMode};
use tagjoe_core::model::{Field, Kind};
use tagjoe_core::naming::Context;
use tagjoe_core::propose;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Templates,
    Sources,
    Clean,
    Tags,
    Audiobook,
    General,
}

pub struct SettingsState {
    pub cfg: Config,
    pub page: Page,
    pub keep_text: String,
    pub regions_text: String,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    Page(Page),
    Album(String),
    Compilation(String),
    Audiobook(String),
    FolderEnabled(bool),
    FolderAlbum(String),
    FolderCompilation(String),
    FolderAudiobook(String),
    FolderPreset(&'static str),
    Pad(bool),
    DiscMode(DiscMode),
    DiscSegment(String),
    AcoustIdKey(String),
    DiscogsEnabled(bool),
    DiscogsToken(String),
    AudiobookDbKey(String),
    KeepText(String),
    RemoveSecondary(bool),
    Id3Version(String),
    CoverMax(String),
    CoverMin(String),
    ChapterTitle(String),
    Regions(String),
    BookLanguage(String),
    Language(String),
    CacheDays(String),
    Reset,
    Save,
    Close,
}

pub enum Outcome {
    Keep,
    Close,
    Save(Config),
}

impl SettingsState {
    pub fn new(cfg: Config) -> Self {
        let keep_text = cfg.tags.keep_fields.join("\n");
        let regions_text = cfg.audiobook.audnexus_regions.join(", ");
        Self { cfg, page: Page::Templates, keep_text, regions_text }
    }

    pub fn update(&mut self, m: SettingsMessage) -> Outcome {
        use SettingsMessage as M;
        match m {
            M::Page(p) => self.page = p,
            M::Album(s) => self.cfg.templates.album = s,
            M::Compilation(s) => self.cfg.templates.compilation = s,
            M::Audiobook(s) => self.cfg.templates.audiobook = s,
            M::FolderEnabled(b) => self.cfg.templates.folder_enabled = b,
            M::FolderAlbum(s) => self.cfg.templates.folder_album = s,
            M::FolderCompilation(s) => self.cfg.templates.folder_compilation = s,
            M::FolderAudiobook(s) => self.cfg.templates.folder_audiobook = s,
            M::FolderPreset(p) => match p {
                "jellyfin" => {
                    self.cfg.templates.folder_album = "{albumartist}/{album}".into();
                    self.cfg.templates.folder_compilation = "Various Artists/{album}".into();
                    self.cfg.templates.folder_audiobook = "{author}/{book}".into();
                }
                _ => {
                    self.cfg.templates.folder_album = tagjoe_core::config::DEFAULT_FOLDER_ALBUM.into();
                    self.cfg.templates.folder_compilation = tagjoe_core::config::DEFAULT_FOLDER_COMPILATION.into();
                    self.cfg.templates.folder_audiobook = tagjoe_core::config::DEFAULT_FOLDER_AUDIOBOOK.into();
                }
            },
            M::Pad(b) => self.cfg.numbering.pad = b,
            M::DiscMode(d) => self.cfg.numbering.disc_mode = d,
            M::DiscSegment(s) => self.cfg.numbering.disc_segment = s,
            M::AcoustIdKey(s) => self.cfg.sources.acoustid_key = s,
            M::DiscogsEnabled(b) => self.cfg.sources.discogs_enabled = b,
            M::DiscogsToken(s) => self.cfg.sources.discogs_token = s,
            M::AudiobookDbKey(s) => self.cfg.sources.audiobookdb_key = s,
            M::KeepText(s) => {
                self.cfg.tags.keep_fields = s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
                self.keep_text = s;
            }
            M::RemoveSecondary(b) => self.cfg.tags.remove_secondary_containers = b,
            M::Id3Version(s) => self.cfg.tags.id3_version = s,
            M::CoverMax(s) => {
                if let Ok(n) = s.trim().parse() {
                    self.cfg.tags.cover_max_px = n;
                }
            }
            M::CoverMin(s) => {
                if let Ok(n) = s.trim().parse() {
                    self.cfg.tags.cover_min_px = n;
                }
            }
            M::ChapterTitle(s) => self.cfg.audiobook.chapter_title = s,
            M::Regions(s) => {
                self.cfg.audiobook.audnexus_regions = s.split(',').map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty()).collect();
                self.regions_text = s;
            }
            M::BookLanguage(s) => self.cfg.audiobook.language = s,
            M::Language(s) => self.cfg.language = s,
            M::CacheDays(s) => {
                if let Ok(n) = s.trim().parse() {
                    self.cfg.sources.cache_days = n;
                }
            }
            M::Reset => {
                *self = SettingsState::new(Config::default());
            }
            M::Save => return Outcome::Save(self.cfg.clone()),
            M::Close => return Outcome::Close,
        }
        Outcome::Keep
    }
}

/// Sample context for previews: the selected job's first file, else a built-in example.
fn sample_context(app: &App, kind: Kind, cfg: &Config) -> Context {
    if let Some(j) = app.selected.and_then(|i| app.jobs.get(i)) {
        if j.unit.kind == kind {
            if let Some(p) = &j.plan {
                if let (Some(f), Some(fi)) = (j.unit.files.first(), p.files.first()) {
                    let mut merged = std::collections::BTreeMap::new();
                    for c in &fi.changes {
                        merged.insert(c.field, if c.new.is_empty() { c.old.clone() } else { c.new.clone() });
                    }
                    let track: u32 = merged.get(&Field::TrackNumber).and_then(|s| s.parse().ok()).unwrap_or(1);
                    let total: u32 = merged.get(&Field::TrackTotal).and_then(|s| s.parse().ok()).unwrap_or(j.unit.files.len() as u32);
                    return propose::context_for(kind, &merged, &f.ext, (track, total, 1, 1), cfg);
                }
            }
        }
    }
    let mut m = std::collections::BTreeMap::new();
    match kind {
        Kind::Audiobook => {
            m.insert(Field::AlbumArtist, "Jo Nesbø".into());
            m.insert(Field::Album, "Der Fledermausmann".into());
            m.insert(Field::Date, "2011".into());
            m.insert(Field::Title, "Kapitel 01".into());
            m.insert(Field::Series, "Harry Hole".into());
            m.insert(Field::SeriesPart, "1".into());
            m.insert(Field::Narrator, "Uve Teschner".into());
            propose::context_for(kind, &m, "mp3", (1, 14, 1, 1), cfg)
        }
        Kind::Compilation => {
            m.insert(Field::AlbumArtist, "Various Artists".into());
            m.insert(Field::Album, "Anjunadeep 16".into());
            m.insert(Field::Date, "2026-10-24".into());
            m.insert(Field::Artist, "Kloyd & Fractures".into());
            m.insert(Field::Title, "Will We Ever Learn (Mixed)".into());
            propose::context_for(kind, &m, "mp3", (1, 41, 1, 1), cfg)
        }
        _ => {
            m.insert(Field::AlbumArtist, "Jody Wisternoff".into());
            m.insert(Field::Artist, "Jody Wisternoff".into());
            m.insert(Field::Album, "Anjunadeep 09".into());
            m.insert(Field::Date, "2017".into());
            m.insert(Field::Title, "Wherever You Go".into());
            propose::context_for(kind, &m, "flac", (3, 20, 1, 2), cfg)
        }
    }
}

fn preview(app: &App, kind: Kind, cfg: &Config) -> String {
    let ctx = sample_context(app, kind, cfg);
    match propose::render_name(kind, &ctx, if kind == Kind::Album { 2 } else { 1 }, cfg) {
        Ok(n) => format!("{} {n}", t("Preview:")),
        Err(e) => format!("{} {e}", t("Error:")),
    }
}

fn card<'a>(title: String, body: Element<'a, SettingsMessage>) -> Element<'a, SettingsMessage> {
    container(column![text(title).size(12).style(th::dim), body].spacing(6)).padding(10).width(Fill).style(th::card).into()
}

fn field<'a>(label: &str, value: &str, on: impl Fn(String) -> SettingsMessage + 'a) -> Element<'a, SettingsMessage> {
    row![text(label.to_string()).size(13).width(Length::Fixed(200.0)), text_input("", value).on_input(on).size(13).padding(5).width(Fill)].spacing(8).align_y(iced::Center).into()
}

pub fn view<'a>(s: &'a SettingsState, app: &'a App) -> Element<'a, SettingsMessage> {
    let cfg = &s.cfg;
    let nav = {
        let mut c = column![].spacing(4);
        for (p, label) in [(Page::Templates, t("Templates")), (Page::Sources, t("Sources and keys")), (Page::Clean, t("Clean-up")), (Page::Tags, t("Tags")), (Page::Audiobook, t("Audiobooks")), (Page::General, t("General"))] {
            c = c.push(button(text(label).size(13)).on_press(SettingsMessage::Page(p)).style(th::list_button(s.page == p)).width(Fill).padding(8));
        }
        container(c.padding(8)).width(220).height(Fill).style(th::sidebar)
    };
    let body: Element<'a, SettingsMessage> = match s.page {
        Page::Templates => column![
            card(t("FILE NAME · ALBUM"), column![text_input("", &cfg.templates.album).on_input(SettingsMessage::Album).font(iced::Font::MONOSPACE).size(13).padding(6), text(preview(app, Kind::Album, cfg)).size(12).style(th::dim)].spacing(4).into()),
            card(t("FILE NAME · COMPILATION"), column![text_input("", &cfg.templates.compilation).on_input(SettingsMessage::Compilation).font(iced::Font::MONOSPACE).size(13).padding(6), text(preview(app, Kind::Compilation, cfg)).size(12).style(th::dim)].spacing(4).into()),
            card(t("FILE NAME · AUDIOBOOK"), column![text_input("", &cfg.templates.audiobook).on_input(SettingsMessage::Audiobook).font(iced::Font::MONOSPACE).size(13).padding(6), text(preview(app, Kind::Audiobook, cfg)).size(12).style(th::dim)].spacing(4).into()),
            row![
                card(t("FOLDER"), column![
                    toggler(cfg.templates.folder_enabled).label(t("Rename folders (off when Jellyfin or Plex dictate the structure)")).on_toggle(SettingsMessage::FolderEnabled).text_size(13),
                    row![button(text(t("Preset: flat")).size(12)).on_press(SettingsMessage::FolderPreset("flat")).style(button::secondary).padding([4, 8]), button(text(t("Preset: Jellyfin/Plex")).size(12)).on_press(SettingsMessage::FolderPreset("jellyfin")).style(button::secondary).padding([4, 8])].spacing(6),
                    field(&t("Album"), &cfg.templates.folder_album, SettingsMessage::FolderAlbum),
                    field(&t("Compilation"), &cfg.templates.folder_compilation, SettingsMessage::FolderCompilation),
                    field(&t("Audiobook"), &cfg.templates.folder_audiobook, SettingsMessage::FolderAudiobook),
                ].spacing(6).into()),
                card(t("DISCS AND NUMBERS"), column![
                    radio(t("Number continuously across discs (14 of 27)"), DiscMode::Continuous, Some(cfg.numbering.disc_mode), SettingsMessage::DiscMode).size(16).text_size(13),
                    radio(t("Disc segment in the name (CD2 von 2 - 03 von 12)"), DiscMode::Segment, Some(cfg.numbering.disc_mode), SettingsMessage::DiscMode).size(16).text_size(13),
                    field(&t("Disc segment"), &cfg.numbering.disc_segment, SettingsMessage::DiscSegment),
                    toggler(cfg.numbering.pad).label(t("Pad numbers to the width of the total (03 of 12, 007 of 120)")).on_toggle(SettingsMessage::Pad).text_size(13),
                    text(t("Characters forbidden on Windows (\\ / : * ? \" < > |) are always replaced; names over 255 bytes are shortened.")).size(12).style(th::dim),
                ].spacing(8).into()),
            ].spacing(10),
            card(t("SYNTAX"), text(t("{field} placeholder · {field.pad(3)} · .upper() .lower() .trim() · .replace(a, b) · .or(x) fallback · {disctotal > 1 ? 'CD' + disc + ' - ' : ''} condition\nFields: albumartist, artist, album, year, date, track, tracktotal, disc, disctotal, title, author, book, chapter, chaptertotal, narrator, series, part, label, catalog, barcode, genre, ext")).size(12).style(th::dim).into()),
        ].spacing(10).into(),
        Page::Sources => column![
            card(t("MUSICBRAINZ AND ACOUSTID"), column![
                text(t("MusicBrainz and the Cover Art Archive need no key. AcoustID fingerprint lookups need an application key: register tagjoe at https://acoustid.org/new-application and paste the key here. fpcalc (libchromaprint-tools) must be installed.")).size(12).style(th::dim),
                field(&t("AcoustID application key"), &cfg.sources.acoustid_key, SettingsMessage::AcoustIdKey),
            ].spacing(6).into()),
            card(t("DISCOGS"), column![
                toggler(cfg.sources.discogs_enabled).label(t("Use Discogs as a second music source (personal access token from discogs.com/settings/developers)")).on_toggle(SettingsMessage::DiscogsEnabled).text_size(13),
                field(&t("Discogs token"), &cfg.sources.discogs_token, SettingsMessage::DiscogsToken),
            ].spacing(6).into()),
            card(t("AUDIOBOOKS"), column![
                text(t("Audiobook data from AudiobookDB (https://audiobookdb.org) and Audnexus. Anonymous access works; an AudiobookDB API key (free account) raises the limits.")).size(12).style(th::dim),
                field(&t("AudiobookDB API key"), &cfg.sources.audiobookdb_key, SettingsMessage::AudiobookDbKey),
            ].spacing(6).into()),
            card(t("CACHE"), field(&t("Keep responses for (days)"), &cfg.sources.cache_days.to_string(), SettingsMessage::CacheDays)),
        ].spacing(10).into(),
        Page::Clean => column![
            card(t("FOREIGN FIELDS"), column![
                text(t("Every tag field outside the schema is foreign and removed by Apply or Clean. List the raw names to keep, one per line; * is a wildcard (e.g. REPLAYGAIN_*, TXXX:MyRating).")).size(12).style(th::dim),
                text_input(&t("one pattern per line"), &s.keep_text).on_input(SettingsMessage::KeepText).font(iced::Font::MONOSPACE).size(13).padding(6),
            ].spacing(6).into()),
            card(t("SECONDARY CONTAINERS"), toggler(cfg.tags.remove_secondary_containers).label(t("Remove ID3v1 and APE containers from MP3 files (ID3v2 stays)")).on_toggle(SettingsMessage::RemoveSecondary).text_size(13).into()),
        ].spacing(10).into(),
        Page::Tags => column![
            card(t("ID3"), column![
                radio(t("ID3v2.4 (UTF-8, standard)"), "2.4", Some(cfg.tags.id3_version.as_str()), |v: &str| SettingsMessage::Id3Version(v.to_string())).size(16).text_size(13),
                radio(t("ID3v2.3 (older Windows software)"), "2.3", Some(cfg.tags.id3_version.as_str()), |v: &str| SettingsMessage::Id3Version(v.to_string())).size(16).text_size(13),
            ].spacing(6).into()),
            card(t("COVER"), column![
                field(&t("Scale embedded covers down to (px)"), &cfg.tags.cover_max_px.to_string(), SettingsMessage::CoverMax),
                field(&t("Replace existing covers smaller than (px)"), &cfg.tags.cover_min_px.to_string(), SettingsMessage::CoverMin),
            ].spacing(6).into()),
        ].spacing(10).into(),
        Page::Audiobook => column![
            card(t("CHAPTERS"), column![
                field(&t("Title for unnamed chapters"), &cfg.audiobook.chapter_title, SettingsMessage::ChapterTitle),
                text(t("{chapter} and {chaptertotal} are available, e.g. \"Kapitel {chapter}\" or \"Chapter {chapter} of {chaptertotal}\".")).size(12).style(th::dim),
            ].spacing(6).into()),
            card(t("SOURCES"), column![
                field(&t("Audible regions (Audnexus), in order"), &s.regions_text, SettingsMessage::Regions),
                field(&t("Preferred release language (AudiobookDB)"), &cfg.audiobook.language, SettingsMessage::BookLanguage),
            ].spacing(6).into()),
        ].spacing(10).into(),
        Page::General => column![
            card(t("LANGUAGE"), column![field(&t("UI language (empty = system)"), &cfg.language, SettingsMessage::Language), text(t("Base language is English; German is available.")).size(12).style(th::dim)].spacing(6).into()),
            card(t("CONFIGURATION FILE"), text(tagjoe_core::config::config_path().display().to_string()).size(12).font(iced::Font::MONOSPACE).into()),
            card(t("CREDITS"), text(t("Music data: MusicBrainz, AcoustID, Cover Art Archive, Discogs. Audiobook data: AudiobookDB (https://audiobookdb.org) and Audnexus.")).size(12).style(th::dim).into()),
        ].spacing(10).into(),
    };
    let header = container(row![text(t("tagjoe – Settings")).size(16), space().width(Fill), button(text(t("Reset to defaults")).size(13)).on_press(SettingsMessage::Reset).style(button::secondary).padding([6, 10]), button(text(t("Cancel")).size(13)).on_press(SettingsMessage::Close).style(button::secondary).padding([6, 10]), button(text(t("Save")).size(13)).on_press(SettingsMessage::Save).style(button::primary).padding([6, 12])].spacing(8).align_y(iced::Center).padding([6, 12])).width(Fill).style(th::header_bar);
    column![header, row![nav, scrollable(container(body).padding(10)).height(Fill).width(Fill)].height(Fill)].into()
}
