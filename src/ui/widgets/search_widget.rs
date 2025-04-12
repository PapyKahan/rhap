use ratatui::{
    prelude::Rect,
    style::Style,
    widgets::{Clear, Paragraph},
    Frame,
};

use crate::ui::HIGHLIGHT_COLOR;

pub struct SearchWidget {
    input: String,
    cursor_position: usize,
    search_result_index: Option<usize>,
    last_query: String, // Track the last query for next match functionality
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

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn search_result(&self) -> Option<usize> {
        self.search_result_index
    }

    pub fn handle_input(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    pub fn handle_backspace(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
        }
    }

    // New method to handle the Delete key
    pub fn handle_delete(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
        }
    }

    // Move cursor to the left
    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    // Move cursor to the right
    pub fn move_cursor_right(&mut self) {
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
        // Save the current input as the last query when a result is found
        if index.is_some() {
            self.last_query = self.input.clone();
        }
    }

    // Add this method to get the last search query
    pub fn last_query(&self) -> &str {
        &self.last_query
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        // Create a search area at the very bottom of the screen
        let search_area = Rect {
            x: area.x,
            y: area.y + area.height - 1, // Position at the very bottom line
            width: area.width,           // Use full width
            height: 1,                   // Just 1 line high like vim
        };

        // Create separate spans for icon and input text with different colors
        let search_text = ratatui::text::Text::from(ratatui::text::Line::from(vec![
            ratatui::text::Span::styled("", Style::default().fg(HIGHLIGHT_COLOR)),
            ratatui::text::Span::raw(" "), // Space between icon and input
            ratatui::text::Span::styled(
                &self.input,
                Style::default().fg(ratatui::style::Color::White),
            ),
        ]));

        // Simple paragraph without borders for a vim-like look
        let paragraph = Paragraph::new(search_text);

        frame.render_widget(Clear, search_area);
        frame.render_widget(paragraph, search_area);

        // Position cursor right after the '/' plus the current input position
        // Position cursor right after the '/' plus the current input position
        frame.set_cursor_position((
            search_area.x + self.cursor_position as u16 + 2, // +1 for '/' and +1 for space
            search_area.y,
        ));
    }
}
