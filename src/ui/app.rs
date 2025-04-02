use super::{screens::Playlist, utils::bottom_right_fixed_size, widgets::DeviceSelector, events::{KeyboardManager, KeyboardEvent}};
use crate::{audio::Host, player::Player};
use anyhow::Result;
use crossterm::event::{self};
use crossterm::terminal::SetTitle;
use crossterm::ExecutableCommand;
use log::error;
use ratatui::{DefaultTerminal, Frame};
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use tokio::sync::broadcast;

pub enum Screens {
    OutputSelector(Rc<RefCell<DeviceSelector>>),
    Default(Rc<RefCell<Playlist>>),
}

pub struct App {
    layers: Vec<Screens>,
    output_selector: Rc<RefCell<DeviceSelector>>,
    playlist: Rc<RefCell<Playlist>>,
    keyboard_manager: KeyboardManager,
    event_receiver: broadcast::Receiver<KeyboardEvent>,
}

impl App {
    pub fn new(host: Host, player: Player, path: PathBuf) -> Result<Self> {
        let keyboard_manager = KeyboardManager::new();
        let event_receiver = keyboard_manager.get_receiver();
        
        Ok(Self {
            layers: vec![],
            output_selector: Rc::new(RefCell::new(DeviceSelector::new(host)?)),
            playlist: Rc::new(RefCell::new(Playlist::new(path, player)?)),
            keyboard_manager,
            event_receiver,
        })
    }

    fn render(&mut self, frame: &mut Frame) -> Result<()> {
        self.playlist.borrow_mut().render(frame, frame.area())?;
        let layer = if self.layers.is_empty() {
            return Ok(());
        } else {
            self.layers.last().unwrap()
        };
        match layer {
            Screens::OutputSelector(selector) => {
                let area = bottom_right_fixed_size(40, 6, frame.area());
                (*selector).borrow_mut().render(frame, area)?;
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> Result<()> {
        let default_screen = Screens::Default(self.playlist.clone());
        let current_screen = self.layers.last().unwrap_or(&default_screen);
        
        match event {
            KeyboardEvent::Quit => {
                if let Screens::Default(playlist) = current_screen {
                    playlist.borrow_mut().stop().await?;
                }
                return Ok(());
            }
            KeyboardEvent::DeviceSelector => {
                self.output_selector.borrow_mut().refresh_device_list()?;
                self.layers.push(Screens::OutputSelector(self.output_selector.clone()));
            }
            _ => {
                match current_screen {
                    Screens::OutputSelector(selector) => {
                        match event {
                            KeyboardEvent::Up => selector.borrow_mut().select_previous(),
                            KeyboardEvent::Down => selector.borrow_mut().select_next(),
                            KeyboardEvent::Enter => selector.borrow_mut().set_selected_device()?,
                            KeyboardEvent::Quit => {
                                self.layers.pop();
                            }
                            _ => {}
                        }
                    }
                    Screens::Default(playlist) => {
                        match event {
                            KeyboardEvent::Play => playlist.borrow_mut().play_selected().await?,
                            KeyboardEvent::Pause => {
                                if playlist.borrow_mut().is_playing() {
                                    playlist.borrow_mut().pause().await?;
                                } else {
                                    playlist.borrow_mut().resume().await?;
                                }
                            },
                            KeyboardEvent::Stop => playlist.borrow_mut().stop().await?,
                            KeyboardEvent::Next => playlist.borrow_mut().next().await?,
                            KeyboardEvent::Previous => playlist.borrow_mut().previous().await?,
                            KeyboardEvent::Up => playlist.borrow_mut().select_previous(),
                            KeyboardEvent::Down => playlist.borrow_mut().select_next(),
                            KeyboardEvent::Enter => playlist.borrow_mut().play_selected().await?,
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal
            .backend_mut()
            .execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;
        
        loop {
            terminal.draw(|frame| match self.render(frame) {
                Ok(ok) => ok,
                Err(err) => {
                    error!("error while drawing {}", err.to_string());
                    ()
                }
            })?;

            // Gestion des événements clavier
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Ok(event) = event::read() {
                    self.keyboard_manager.handle_event(event).await?;
                }
            }

            // Traitement des événements clavier
            while let Ok(event) = self.event_receiver.try_recv() {
                self.handle_keyboard_event(&event).await?;
                if let KeyboardEvent::Quit = event {
                    return Ok(());
                }
            }

            // Mise à jour de l'interface
            let default_screen = Screens::Default(self.playlist.clone());
            let current_screen = self.layers.last().unwrap_or(&default_screen);
            match current_screen {
                Screens::Default(playlist) => {
                    playlist.borrow_mut().run().await?;
                }
                _ => {}
            }
        }
    }
}
