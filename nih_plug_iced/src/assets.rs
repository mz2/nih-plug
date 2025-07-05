//! Binary assets for use with `nih_plug_iced`.

use std::borrow::Cow;

use crate::core::Font;

use iced_baseview::font::Family::*;
use iced_baseview::font::*;
// This module provides a re-export and simple font wrappers around the re-exported fonts.
pub use nih_plug_assets::*;

pub const NOTO_SANS_REGULAR: Font = Font::with_name("Noto Sans");
pub const NOTO_SANS_REGULAR_ITALIC: Font = Font::with_name("Noto Sans");
pub const NOTO_SANS_THIN: Font = Font::with_name("Noto Sans Thin");
pub const NOTO_SANS_THIN_ITALIC: Font = Font::with_name("Noto Sans Thin Italic");
pub const NOTO_SANS_LIGHT: Font = Font::with_name("Noto Sans Light");
pub const NOTO_SANS_LIGHT_ITALIC: Font = Font::with_name("Noto Sans Light Italic");
pub const NOTO_SANS_BOLD: Font = Font {
    family: Name("Noto Sans"),
    weight: Weight::Bold,
    stretch: Stretch::Normal,
    style: Style::Normal,
};
pub const NOTO_SANS_BOLD_ITALIC: Font = Font::with_name("Noto Sans Bold Italic");

pub const NOTO_SANS_MONO_REGULAR: Font = Font::with_name("Noto Sans Mono");
pub const NOTO_SANS_MONO_BOLD: Font = Font::with_name("Noto Sans Mono Bold");

pub const COURIER_NEW: Font = Font::with_name("Courier New");

/// Useful for initializing the Settings, like this:
/// ```rust,ignore
///    Settings {
///        ...
///        fonts: noto_sans_fonts_data().into_iter().collect(),
///    }
/// ```
pub const fn noto_sans_fonts_data() -> [Cow<'static, [u8]>; 8] {
    [
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_REGULAR),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_REGULAR_ITALIC),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_THIN),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_THIN_ITALIC),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_LIGHT),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_LIGHT_ITALIC),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_BOLD),
        Cow::Borrowed(nih_plug_assets::fonts::NOTO_SANS_BOLD_ITALIC),
    ]
}
