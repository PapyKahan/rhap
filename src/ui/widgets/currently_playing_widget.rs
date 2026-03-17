use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Alignment, Line, Rect, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
    Frame,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, StatefulImage};
use std::sync::Arc;

use crate::{player::format_time, ui::component::RenderContext};

struct CachedCoverArt {
    data_id: usize,
    protocol: StatefulProtocol,
}

pub struct CurrentlyPlayingWidget {
    cached_image: Option<CachedCoverArt>,
    picker: Option<Picker>,
}

impl CurrentlyPlayingWidget {
    pub fn new(picker: Option<Picker>) -> Self {
        Self {
            cached_image: None,
            picker,
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &RenderContext) {
        let block = Block::default()
            .title("Currently Playing")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(ctx.theme.border)
            .padding(Padding::uniform(1));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let has_cover = ctx
            .playing_track
            .and_then(|t| t.cover_art.as_ref())
            .is_some()
            && self.picker.is_some();

        if has_cover {
            // Image width ≈ height*2 (terminal cells are ~2:1), capped at 1/3 of width
            let img_width = (inner.height as u16 * 2).min(inner.width / 3);
            let layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(img_width), Constraint::Min(1)])
                .split(inner);

            self.render_cover_art(frame, layout[0], ctx);
            self.render_text(frame, layout[1], ctx);
        } else {
            self.cached_image = None;
            self.render_text(frame, inner, ctx);
        }
    }

    fn render_cover_art(&mut self, frame: &mut Frame, area: Rect, ctx: &RenderContext) {
        let track = match ctx.playing_track {
            Some(t) => t,
            None => return,
        };
        let data = match &track.cover_art {
            Some(d) => d,
            None => return,
        };
        let picker = match &mut self.picker {
            Some(p) => p,
            None => return,
        };

        let data_id = Arc::as_ptr(data) as *const u8 as usize;

        // Only decode on track change
        let needs_decode = match &self.cached_image {
            Some(cached) => cached.data_id != data_id,
            None => true,
        };

        if needs_decode {
            match image::load_from_memory(data) {
                Ok(img) => {
                    let protocol = picker.new_resize_protocol(img);
                    self.cached_image = Some(CachedCoverArt {
                        data_id,
                        protocol,
                    });
                }
                Err(e) => {
                    log::warn!("Failed to decode cover art: {}", e);
                    self.cached_image = None;
                    return;
                }
            }
        }

        if let Some(cached) = &mut self.cached_image {
            let image_widget = StatefulImage::new();
            frame.render_stateful_widget(image_widget, area, &mut cached.protocol);
        }
    }

    fn render_text(&self, frame: &mut Frame, area: Rect, ctx: &RenderContext) {
        let elapsed_time = ctx
            .playing_track
            .map(|t| t.get_elapsed_time())
            .unwrap_or_default();

        if let Some(track_info) = ctx.playing_track {
            // Split: info lines top-aligned, progress bar pinned to bottom
            let info_area = Rect { height: area.height.saturating_sub(1), ..area };
            let progress_area = Rect {
                y: area.y + area.height.saturating_sub(1),
                height: 1,
                ..area
            };

            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Title: ", ctx.theme.text_bold),
                    Span::styled(&track_info.title, ctx.theme.text),
                ]),
                Line::from(vec![
                    Span::styled("Artist: ", ctx.theme.text_bold),
                    Span::styled(&track_info.artist, ctx.theme.text),
                ]),
                Line::from(vec![
                    Span::styled("Info: ", ctx.theme.text_bold),
                    Span::styled(format!("{}", track_info.info), ctx.theme.text),
                ]),
            ];
            if let Some(output) = &track_info.output_info {
                lines.push(Line::from(vec![
                    Span::styled("Playing as: ", ctx.theme.text_bold),
                    Span::styled(output, ctx.theme.text),
                ]));
            }

            let info = Paragraph::new(lines).alignment(Alignment::Center);
            frame.render_widget(info, info_area);

            let progress = if track_info.total_duration.seconds > 0 {
                (elapsed_time.seconds as f64 / track_info.total_duration.seconds as f64) * 100.0
            } else {
                0.0
            };
            let progress_bar_width = (progress_area.width as usize).saturating_sub(20);
            let filled_width = ((progress / 100.0) * progress_bar_width as f64).round() as usize;
            let empty_width = progress_bar_width.saturating_sub(filled_width);

            let progress_line = Line::from(vec![
                Span::styled(format_time(elapsed_time), ctx.theme.text),
                Span::raw(" "),
                Span::styled(
                    "".repeat(filled_width),
                    ctx.theme.progress.filled,
                ),
                Span::styled(
                    "",
                    ctx.theme.progress.cursor,
                ),
                Span::styled(
                    "".repeat(empty_width),
                    ctx.theme.progress.empty,
                ),
                Span::raw(" "),
                Span::styled(format_time(track_info.total_duration), ctx.theme.text),
            ]);

            let bar = Paragraph::new(progress_line).alignment(Alignment::Center);
            frame.render_widget(bar, progress_area);
        } else if let Some(msg) = ctx.status_message {
            let paragraph = Paragraph::new(Line::from(Span::styled(msg, ctx.theme.error)))
                .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        } else {
            let paragraph = Paragraph::new("No track playing").alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        };
    }
}
