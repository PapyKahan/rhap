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
}

pub struct KeyboardManager {
    sender: Sender<KeyboardEvent>,
    receiver: broadcast::Receiver<KeyboardEvent>,
    search_mode: bool,  // Nouvel attribut pour suivre l'état du mode recherche
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

    // Ajouter des méthodes pour activer/désactiver le mode recherche
    pub fn set_search_mode(&mut self, active: bool) {
        self.search_mode = active;
    }

    pub async fn handle_event(&self, event: Event) -> Result<()> {
        if let Event::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                let keyboard_event = if self.search_mode {
                    // En mode recherche, les caractères sont traités différemment mais les autres touches restent normales
                    match key.code {
                        // Ces touches conservent leur comportement spécial même en mode recherche
                        KeyCode::Enter => Some(KeyboardEvent::Enter),
                        KeyCode::Esc => Some(KeyboardEvent::Escape),
                        KeyCode::Backspace => Some(KeyboardEvent::Backspace),
                        KeyCode::Delete => Some(KeyboardEvent::Delete),
                        KeyCode::Left => Some(KeyboardEvent::Left),
                        KeyCode::Right => Some(KeyboardEvent::Right),
                        
                        // Les caractères sont envoyés comme entrée de texte en mode recherche
                        KeyCode::Char(c) => Some(KeyboardEvent::Char(c)),
                        
                        // Autres touches ignorées en mode recherche
                        _ => None,
                    }
                } else {
                    // Comportement normal hors mode recherche
                    match key.code {
                        KeyCode::Enter => Some(KeyboardEvent::Enter),
                        KeyCode::Char('p') => Some(KeyboardEvent::Play),
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