use ratatui::style::{Color, Modifier, Style};

/// Raw named colors from a color scheme.
pub struct Palette {
    pub text: Color,
    pub red: Color,
    pub blue: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub mantle: Color,
}

impl Palette {
    pub fn catppuccin_mocha() -> Self {
        Self {
            text: Color::Rgb(0xcd, 0xd6, 0xf4),
            red: Color::Rgb(0xf3, 0x8b, 0xa8),
            blue: Color::Rgb(0x89, 0xb4, 0xfa),
            surface0: Color::Rgb(0x31, 0x32, 0x44),
            surface1: Color::Rgb(0x45, 0x47, 0x5a),
            surface2: Color::Rgb(0x58, 0x5b, 0x70),
            mantle: Color::Rgb(0x18, 0x18, 0x25),
        }
    }
}

pub struct TableStyles {
    pub row_even: Style,
    pub row_odd: Style,
    pub cell_even: Style,
    pub cell_odd: Style,
    pub highlight: Style,
}

pub struct ProgressBarStyles {
    pub filled: Style,
    pub cursor: Style,
    pub empty: Style,
}

pub struct ScrollbarStyles {
    pub track: Style,
    pub thumb: Style,
}

pub struct Theme {
    pub text: Style,
    pub text_bold: Style,
    pub accent: Style,
    pub border: Style,
    pub error: Style,
    pub table: TableStyles,
    pub progress: ProgressBarStyles,
    pub scrollbar: ScrollbarStyles,
}

impl Theme {
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            text: Style::default().fg(p.text),
            text_bold: Style::default().fg(p.text).add_modifier(Modifier::BOLD),
            accent: Style::default().fg(p.blue),
            border: Style::default().fg(p.blue),
            error: Style::default().fg(p.red),
            table: TableStyles {
                row_even: Style::default().bg(p.surface0),
                row_odd: Style::default().bg(p.mantle),
                cell_even: Style::default().bg(p.surface1),
                cell_odd: Style::default().bg(p.surface0),
                highlight: Style::default().fg(p.blue),
            },
            progress: ProgressBarStyles {
                filled: Style::default().fg(p.blue).add_modifier(Modifier::BOLD),
                cursor: Style::default().fg(p.blue).add_modifier(Modifier::BOLD),
                empty: Style::default().fg(p.surface2).add_modifier(Modifier::BOLD),
            },
            scrollbar: ScrollbarStyles {
                track: Style::default().fg(p.surface1),
                thumb: Style::default().fg(p.blue),
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_palette(&Palette::catppuccin_mocha())
    }
}
