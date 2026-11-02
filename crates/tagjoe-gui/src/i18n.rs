//! Translation shorthand for the GUI (gettext catalogues live in tagjoe-core).

pub fn t(s: &str) -> String {
    tagjoe_core::i18n::tr(s)
}
