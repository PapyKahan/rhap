use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use tokio::sync::broadcast::{self, Sender};

#[derive(Debug, Clone)]
pub enum KeyboardEvent {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Quit,
    DeviceSelector,
    Search,
    Escape,
    Up,
    Down,
    Enter,
    Backspace,
    Char(char),
    Delete,
    Left,
    Right,
    NextMatch,
    PrevMatch,
}

pub struct KeyboardManager {
    sender: Sender<KeyboardEvent>,
    receiver: broadcast::Receiver<KeyboardEvent>,
    search_mode: bool, // New attribute to track search mode state
}

impl KeyboardManager {
    pub fn new() -> Self {
        let (sender, receiver) = broadcast::channel(100);
        Self {
            sender,
            receiver,
            search_mode: false,
        }
    }

    // Add methods to enable/disable search mode
    pub fn set_search_mode(&mut self, active: bool) {
        self.search_mode = active;
    }

    pub async fn handle_event(&self, event: Event) -> Result<()> {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                let keyboard_event = if self.search_mode {
                    // In search mode, characters are processed differently but other keys remain normal
                    match key.code {
                        // These keys maintain their special behavior even in search mode
                        KeyCode::Enter => Some(KeyboardEvent::Enter),
                        KeyCode::Esc => Some(KeyboardEvent::Escape),
                        KeyCode::Backspace => Some(KeyboardEvent::Backspace),
                        KeyCode::Delete => Some(KeyboardEvent::Delete),
                        KeyCode::Left => Some(KeyboardEvent::Left),
                        KeyCode::Right => Some(KeyboardEvent::Right),

                        // Add CTRL+n support in search mode
                        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            Some(KeyboardEvent::NextMatch)
                        }

                        // Add CTRL+p support in search mode
                        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            Some(KeyboardEvent::PrevMatch)
                        }

                        // Add this line to handle character inputs in search mode
                        KeyCode::Char(c) => Some(KeyboardEvent::Char(c)),

                        // Other keys ignored in search mode
                        _ => None,
                    }
                } else {
                    // Normal behavior outside search mode
                    match key.code {
                        KeyCode::Enter => Some(KeyboardEvent::Enter),
                        KeyCode::Char('p') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            Some(KeyboardEvent::Play)
                        }
                        KeyCode::Char(' ') => Some(KeyboardEvent::Pause),
                        KeyCode::Char('s') => Some(KeyboardEvent::Stop),
                        KeyCode::Char('l') => Some(KeyboardEvent::Next),
                        KeyCode::Char('h') => Some(KeyboardEvent::Previous),
                        KeyCode::Char('q') => Some(KeyboardEvent::Quit),
                        KeyCode::Char('o') => Some(KeyboardEvent::DeviceSelector),
                        KeyCode::Char('/') => Some(KeyboardEvent::Search),
                        KeyCode::Esc => Some(KeyboardEvent::Escape),
                        KeyCode::Up | KeyCode::Char('k') => Some(KeyboardEvent::Up),
                        KeyCode::Down | KeyCode::Char('j') => Some(KeyboardEvent::Down),
                        KeyCode::Backspace => Some(KeyboardEvent::Backspace),
                        KeyCode::Delete => Some(KeyboardEvent::Delete),
                        KeyCode::Left => Some(KeyboardEvent::Left),
                        KeyCode::Right => Some(KeyboardEvent::Right),

                        // Also add CTRL+n support in normal mode
                        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            Some(KeyboardEvent::NextMatch)
                        }

                        // Also add CTRL+p support in normal mode
                        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            Some(KeyboardEvent::PrevMatch)
                        }

                        KeyCode::Char(c) => Some(KeyboardEvent::Char(c)),

                        _ => None,
                    }
                };

                if let Some(event) = keyboard_event {
                    let _ = self.sender.send(event);
                }
            }
        }
        Ok(())
    }

    pub fn get_receiver(&self) -> broadcast::Receiver<KeyboardEvent> {
        self.receiver.resubscribe()
    }
}

