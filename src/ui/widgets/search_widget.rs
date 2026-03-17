use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::Rect,
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::action::Action;
use crate::ui::component::{Component, RenderContext};

pub struct SearchWidget {
    input: String,
    cursor_position: usize,
    search_result_index: Option<usize>,
    last_query: String,
}

impl SearchWidget {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_position: 0,
            search_result_index: None,
            last_query: String::new(),
        }
    }

    fn handle_input(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    fn handle_backspace(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
        }
    }

    fn handle_delete(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
        }
    }

    fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor_position = 0;
        self.search_result_index = None;
    }

    pub fn set_search_result(&mut self, index: Option<usize>) {
        self.search_result_index = index;
        if index.is_some() {
            self.last_query = self.input.clone();
        }
    }

    pub fn last_query(&self) -> &str {
        &self.last_query
    }
}

impl Component for SearchWidget {
    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &RenderContext) -> Result<()> {
        let search_area = Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        };

        let search_text = ratatui::text::Text::from(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled("", ctx.theme.accent),
            ratatui::text::Span::raw(" "),
            ratatui::text::Span::styled(&self.input, ctx.theme.text),
        ]));

        let paragraph = Paragraph::new(search_text);

        frame.render_widget(Clear, search_area);
        frame.render_widget(paragraph, search_area);

        frame.set_cursor_position((
            search_area.x + self.cursor_position as u16 + 2,
            search_area.y,
        ));

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Action> {
        match key.code {
            KeyCode::Esc => Ok(Action::PopLayer),
            KeyCode::Enter => Ok(Action::Batch(vec![
                Action::CommitSearch(self.search_result_index),
                Action::PopLayer,
            ])),
            KeyCode::Backspace => {
                self.handle_backspace();
                Ok(Action::SearchQuery(self.input.clone()))
            }
            KeyCode::Delete => {
                self.handle_delete();
                Ok(Action::SearchQuery(self.input.clone()))
            }
            KeyCode::Left => {
                self.move_cursor_left();
                Ok(Action::None)
            }
            KeyCode::Right => {
                self.move_cursor_right();
                Ok(Action::None)
            }
            // Ctrl+N/P are silently ignored in search mode (consistent with old behavior)
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Ok(Action::None)
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Ok(Action::None)
            }
            KeyCode::Char(c) => {
                self.handle_input(c);
                Ok(Action::SearchQuery(self.input.clone()))
            }
            _ => Ok(Action::None),
        }
    }
}
