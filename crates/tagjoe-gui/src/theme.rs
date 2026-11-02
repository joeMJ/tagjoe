//! Light/dark themes from the user's design standard (gtk-app-design palette) and card styles.

use iced::theme::{Palette, Theme};
use iced::widget::{button, container, text};
use iced::{Border, Color};

pub fn rgb(hex: u32) -> Color {
    Color::from_rgb8(((hex >> 16) & 0xff) as u8, ((hex >> 8) & 0xff) as u8, (hex & 0xff) as u8)
}

pub fn light() -> Theme {
    Theme::custom("tagjoe light", Palette { background: rgb(0xf6f6f6), text: rgb(0x1a1a1a), primary: rgb(0x1c71d8), success: rgb(0x1a7544), warning: rgb(0xa64800), danger: rgb(0xc01c28) })
}

pub fn dark() -> Theme {
    Theme::custom("tagjoe dark", Palette { background: rgb(0x1e1e1e), text: rgb(0xf6f6f6), primary: rgb(0x2a6fcf), success: rgb(0x33d17a), warning: rgb(0xff7800), danger: rgb(0xff8070) })
}

pub struct Colors {
    pub view: Color,
    pub header: Color,
    pub sidebar: Color,
    pub dim: Color,
    pub card_border: Color,
    pub input_border: Color,
    pub selection: Color,
    pub info: Color,
}

pub fn colors(theme: &Theme) -> Colors {
    if theme.extended_palette().is_dark {
        Colors { view: rgb(0x262626), header: rgb(0x2b2b2b), sidebar: rgb(0x171717), dim: rgb(0xc9c8c5), card_border: rgb(0x5c5c62), input_border: rgb(0x8a8a90), selection: rgb(0x2f4466), info: rgb(0x78aeed) }
    } else {
        Colors { view: rgb(0xffffff), header: rgb(0xebebed), sidebar: rgb(0xebebed), dim: rgb(0x5e5c64), card_border: rgb(0xb8b8be), input_border: rgb(0x8a8890), selection: rgb(0xdbe7f7), info: rgb(0x1a5fb4) }
    }
}

/// A card: view background, 1 px border, radius 8.
pub fn card(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style { background: Some(c.view.into()), border: Border { color: c.card_border, width: 1.0, radius: 8.0.into() }, text_color: None, ..Default::default() }
}

pub fn header_bar(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style { background: Some(c.header.into()), border: Border { color: c.card_border, width: 1.0, radius: 0.0.into() }, ..Default::default() }
}

pub fn sidebar(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style { background: Some(c.sidebar.into()), border: Border { color: c.card_border, width: 1.0, radius: 0.0.into() }, ..Default::default() }
}

pub fn drop_zone(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style { background: None, border: Border { color: c.input_border, width: 2.0, radius: 8.0.into() }, text_color: Some(c.dim), ..Default::default() }
}

pub fn dim(theme: &Theme) -> text::Style {
    text::Style { color: Some(colors(theme).dim) }
}

pub fn ok(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().success) }
}
pub fn warn(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().warning) }
}
pub fn err(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().danger) }
}
pub fn info(theme: &Theme) -> text::Style {
    text::Style { color: Some(colors(theme).info) }
}

/// Selectable list entry button (queue, settings navigation).
pub fn list_button(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        let base = if selected { button::Style { background: Some(c.selection.into()), text_color: theme.palette().text, border: Border { color: theme.palette().primary, width: 1.0, radius: 8.0.into() }, ..Default::default() } } else { button::Style { background: Some(c.view.into()), text_color: theme.palette().text, border: Border { color: c.card_border, width: 1.0, radius: 8.0.into() }, ..Default::default() } };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style { border: Border { color: theme.palette().primary, ..base.border }, ..base },
            _ => base,
        }
    }
}
