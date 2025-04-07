use ratatui::{
    prelude::{Alignment, Line, Rect, Span},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::ui::HIGHLIGHT_COLOR;
use crate::{
    player::CurrentTrackInfo,
    ui::{PROGRESSBAR_COLOR, ROW_COLOR},
};
use std::time::{Duration, Instant};
use symphonia::core::units::Time;

pub struct CurrentlyPlayingWidget {
    track_info: Option<CurrentTrackInfo>,
    last_update: Instant,
    last_elapsed_time: Time,
}

impl CurrentlyPlayingWidget {
    pub fn new(track_info: Option<CurrentTrackInfo>) -> Self {
        Self {
            track_info,
            last_update: Instant::now(),
            last_elapsed_time: Time::default(),
        }
    }

    pub fn clear(&mut self) {
        self.track_info = None;
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);

        if elapsed >= Duration::from_millis(100) {
            self.last_update = now;
            if let Some(track_info) = &self.track_info {
                self.last_elapsed_time = track_info.get_elapsed_time();
            }
        }

        let text = if let Some(track_info) = &self.track_info {
            let progress = if track_info.total_duration.seconds > 0 {
                (self.last_elapsed_time.seconds as f64 / track_info.total_duration.seconds as f64)
                    * 100.0
            } else {
                0.0
            };
            let progress_bar_width = (area.width as usize).saturating_sub(20); // Adjust for padding and other elements
            let filled_width = ((progress / 100.0) * progress_bar_width as f64).round() as usize;
            let empty_width = progress_bar_width.saturating_sub(filled_width);

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
                Line::from(vec![
                    Span::raw(track_info.format_time(self.last_elapsed_time)),
                    Span::raw(" "),
                    Span::styled(
                        "".repeat(filled_width),
                        Style::default()
                            .fg(PROGRESSBAR_COLOR)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "",
                        Style::default()
                            .fg(HIGHLIGHT_COLOR)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "".repeat(empty_width),
                        Style::default().fg(ROW_COLOR).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(track_info.format_time(track_info.total_duration)),
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
