use ratatui::{
    prelude::{Alignment, Rect, Span, Line},
    style::{Style, Modifier},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::player::CurrentTrackInfo;
use crate::ui::HIGHLIGHT_COLOR;

pub struct CurrentlyPlayingWidget {
    track_info: Option<CurrentTrackInfo>,
}

impl CurrentlyPlayingWidget {
    pub fn new(track_info: Option<CurrentTrackInfo>) -> Self {
        Self { track_info }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let text = if let Some(track_info) = &self.track_info {
            vec![
                Line::from(vec![
                    Span::styled("Title: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&track_info.title),
                ]),
                Line::from(vec![
                    Span::styled("Artist: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(&track_info.artist),
                ]),
                Line::from(vec![
                    Span::styled("Info: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{}", track_info.info)),
                ]),
            ]
        } else {
            vec![Line::from(Span::raw("No track playing"))]
        };

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .title("Currently Playing")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(HIGHLIGHT_COLOR)),
            )
            .alignment(Alignment::Center);

        frame.render_widget(paragraph, area);
    }
}
