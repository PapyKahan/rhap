use ratatui::style::Color;

mod device_selector;
pub(crate) use device_selector::DeviceSelector;

const ROW_COLOR: Color = Color::Rgb(80, 80, 80);
const ROW_ALTERNATE_COLOR: Color = Color::Rgb(50, 50, 50);
const HIGHLIGHT_COLOR: Color = Color::Rgb(255, 191, 0);
