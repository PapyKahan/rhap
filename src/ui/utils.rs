use ratatui::prelude::{Layout, Direction, Constraint, Rect};

pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn bottom_right_fixed_size(width: u16, height: u16, area: Rect) -> Rect {
    let col = area.width - width;
    let row = area.height - height;
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(row), Constraint::Length(height)])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(col), Constraint::Length(width)])
        .split(popup_layout[1])[1]
}

pub fn centered_fixed_size_rect(width: u16, height: u16, area: Rect) -> Rect {
    let col = (area.width - width) / 2;
    let row = (area.height - height) / 2;
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(row),
            Constraint::Length(height),
            Constraint::Length(row),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(col),
            Constraint::Length(width),
            Constraint::Length(col),
        ])
        .split(popup_layout[1])[1]
}
