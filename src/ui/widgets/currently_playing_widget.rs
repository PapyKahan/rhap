use ratatui::{
    prelude::{Alignment, Line, Rect, Span},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::ui::HIGHLIGHT_COLOR;
use crate::{
    player::format_time,
    ui::{component::RenderContext, PROGRESSBAR_COLOR, ROW_COLOR},
};

const ERROR_COLOR: Color = Color::Rgb(255, 80, 80);

pub struct CurrentlyPlayingWidget;

impl CurrentlyPlayingWidget {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext) {
        let elapsed_time = ctx
            .playing_track
            .map(|t| t.get_elapsed_time())
            .unwrap_or_default();

        let text = if let Some(track_info) = ctx.playing_track {
            let progress = if track_info.total_duration.seconds > 0 {
                (elapsed_time.seconds as f64 / track_info.total_duration.seconds as f64) * 100.0
            } else {
                0.0
            };
            let progress_bar_width = (area.width as usize).saturating_sub(20);
            let filled_width = ((progress / 100.0) * progress_bar_width as f64).round() as usize;
            let empty_width = progress_bar_width.saturating_sub(filled_width);

            let mut lines = vec![
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
            ];
            if let Some(output) = &track_info.output_info {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Playing as: ",
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(output),
                ]));
            }
            lines.push(Line::from(vec![
                Span::raw(format_time(elapsed_time)),
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
                Span::raw(format_time(track_info.total_duration)),
            ]));
            lines
        } else if let Some(msg) = ctx.status_message {
            vec![Line::from(Span::styled(msg, Style::default().fg(ERROR_COLOR)))]
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
