use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{prelude::Rect, Frame};

use crate::action::Action;
use crate::player::CurrentTrackInfo;

/// Read-only state snapshot for terminal rendering.
pub struct RenderContext<'a> {
    pub playing_track: Option<&'a CurrentTrackInfo>,
    pub playing_track_index: usize,
    pub is_playing: bool,
    pub is_paused: bool,
    pub status_message: Option<&'a str>,
}

/// Terminal UI component trait.
pub trait Component {
    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &RenderContext) -> Result<()>;

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Action> {
        let _ = key;
        Ok(Action::None)
    }
}
