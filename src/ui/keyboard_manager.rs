use crossterm::event::{Event, KeyCode, KeyEventKind};
use tokio::sync::broadcast::{self, Sender};
use anyhow::Result;

#[derive(Debug, Clone)]
pub enum KeyboardEvent {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Quit,
    DeviceSelector,
    Search,  // New event for search
    Escape,  // New event for escape
    Up,
    Down,
    Enter,
    Backspace,  // For handling backspace
    Char(char), // For capturing typed characters
    Delete,        // Nouvelle touche Delete
    Left,          // Déplacement du curseur à gauche
    Right,         // Déplacement du curseur à droite
}

pub struct KeyboardManager {
    sender: Sender<KeyboardEvent>,
    receiver: broadcast::Receiver<KeyboardEvent>,
}

impl KeyboardManager {
    pub fn new() -> Self {
        let (sender, receiver) = broadcast::channel(100);
        Self { sender, receiver }
    }

    pub async fn handle_event(&self, event: Event) -> Result<()> {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                let keyboard_event = match key.code {
                    KeyCode::Enter => Some(KeyboardEvent::Enter),
                    KeyCode::Char('p') => Some(KeyboardEvent::Play),
                    KeyCode::Char(' ') => Some(KeyboardEvent::Pause),
                    KeyCode::Char('s') => Some(KeyboardEvent::Stop),
                    KeyCode::Char('l') => Some(KeyboardEvent::Next),
                    KeyCode::Char('h') => Some(KeyboardEvent::Previous),
                    KeyCode::Char('q') => Some(KeyboardEvent::Quit),
                    KeyCode::Char('o') => Some(KeyboardEvent::DeviceSelector),
                    KeyCode::Char('/') => Some(KeyboardEvent::Search),  // New mapping for search
                    KeyCode::Esc => Some(KeyboardEvent::Escape),  // New mapping for escape
                    KeyCode::Up | KeyCode::Char('k') => Some(KeyboardEvent::Up),
                    KeyCode::Down | KeyCode::Char('j') => Some(KeyboardEvent::Down),
                    KeyCode::Backspace => Some(KeyboardEvent::Backspace),
                    KeyCode::Char(c) => Some(KeyboardEvent::Char(c)), // For all other characters
                    KeyCode::Delete => Some(KeyboardEvent::Delete),
                    KeyCode::Left => Some(KeyboardEvent::Left),
                    KeyCode::Right => Some(KeyboardEvent::Right),
                    _ => None,
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