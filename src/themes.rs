use cursive::theme::{Color::Rgb, Palette, PaletteColor::*, Theme};

pub fn dracula() -> Theme {
    let mut palette = Palette::default();

    palette[View] = Rgb(0x28, 0x2A, 0x36);
    palette[Primary] = Rgb(0xF8, 0xF8, 0xF2);
    palette[Secondary] = Rgb(0x62, 0x72, 0xA4);
    palette[Tertiary] = Rgb(0x44, 0x47, 0x5A);
    palette[Shadow] = Rgb(0x21, 0x22, 0x2C);
    palette[TitlePrimary] = palette[Primary];
    palette[TitleSecondary] = palette[Secondary];
    palette[Highlight] = palette[Tertiary];
    palette[HighlightInactive] = palette[Tertiary];
    palette[HighlightText] = palette[Primary];

    Theme {
        palette,
        ..Theme::default()
    }
}
