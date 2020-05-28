use cursive::theme::{
    Theme,
    Palette,
    Color::Rgb,
    PaletteColor::*,
};

pub fn dracula() -> Theme {
    let mut palette = Palette::default();

    palette[View] = Rgb(0x28, 0x2A, 0x36);
    palette[Primary] = Rgb(0xF8, 0xF8, 0xF2);
    palette[Secondary] = Rgb(0x62, 0x72, 0xA4);
    palette[Tertiary] = Rgb(0x44, 0x47, 0x5A);
    palette[Shadow] = Rgb(0x21, 0x22, 0x2C);
    palette[TitlePrimary] = palette[Primary];
    palette[TitleSecondary] = palette[Secondary];
    palette[Highlight] = Rgb(0x8B, 0xE9, 0xFD);
    palette[HighlightInactive] = palette[Tertiary];

    Theme { palette, ..Theme::default() }
}
