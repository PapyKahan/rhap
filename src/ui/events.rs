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
    Up,
    Down,
    Enter,
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
                    KeyCode::Up | KeyCode::Char('k') => Some(KeyboardEvent::Up),
                    KeyCode::Down | KeyCode::Char('j') => Some(KeyboardEvent::Down),
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