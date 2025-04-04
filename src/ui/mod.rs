use ratatui::style::Color;

pub mod app;
pub mod screens;
pub mod utils;
pub mod widgets;
pub mod keyboard_manager;

pub use app::App;
pub use keyboard_manager::{KeyboardManager, KeyboardEvent};

const ROW_COLOR: Color = Color::Rgb(80, 80, 80);
const ROW_COLOR_COL: Color = Color::Rgb(85, 85, 85);
const ROW_ALTERNATE_COLOR: Color = Color::Rgb(50, 50, 50);
const ROW_ALTERNATE_COLOR_COL: Color = Color::Rgb(55, 55, 55);
const HIGHLIGHT_COLOR: Color = Color::Rgb(255, 191, 0);
const PROGRESSBAR_COLOR: Color = Color::Rgb(255, 150, 0);
