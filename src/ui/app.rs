use super::{KeyboardEvent, KeyboardManager};
use super::{screens::Playlist, utils::bottom_right_fixed_size, widgets::{DeviceSelector, SearchWidget}};
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
    SearchWidget(Rc<RefCell<SearchWidget>>),
    Default(Rc<RefCell<Playlist>>),
}

pub struct App {
    layers: Vec<Screens>,
    output_selector: Rc<RefCell<DeviceSelector>>,
    search_widget: Rc<RefCell<SearchWidget>>,
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
            search_widget: Rc::new(RefCell::new(SearchWidget::new())),
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
            Screens::SearchWidget(search) => {
                (*search).borrow_mut().render(frame, frame.area());
            }
            _ => (),
        }
        Ok(())
    }

    async fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> Result<()> {
        let default_screen = Screens::Default(self.playlist.clone());
        let current_screen = self.layers.last().unwrap_or(&default_screen);
        
        match current_screen {
            Screens::SearchWidget(search) => {
                // When search widget is active, only handle search-related keys
                match event {
                    KeyboardEvent::Escape => {
                        // Exit search mode
                        self.layers.pop();
                    },
                    KeyboardEvent::Backspace => {
                        // Handle backspace in search and get the new input string
                        {
                            search.borrow_mut().handle_backspace();
                        } // The mutable borrow ends here
                        
                        // Update search results with a new borrow
                        let query = search.borrow().input().to_string(); // Clone the string to avoid borrowing issues
                        
                        // Now update the search results
                        let index = if !query.is_empty() {
                            self.playlist.borrow().search(&query)
                        } else {
                            None
                        };
                        
                        {
                            search.borrow_mut().set_search_result(index);
                        }
                    },
                    KeyboardEvent::Char(c) => {
                        // Add character to search input
                        {
                            search.borrow_mut().handle_input(*c);
                        } // The mutable borrow ends here
                        
                        // Get a copy of the query
                        let query = search.borrow().input().to_string();
                        
                        // Search for matching items
                        let index = self.playlist.borrow().search(&query);
                        
                        // Update the search result
                        {
                            search.borrow_mut().set_search_result(index);
                        }
                    },
                    KeyboardEvent::Enter => {
                        // Same approach - get data first, then perform actions
                        let search_result = search.borrow().search_result();
                        if let Some(index) = search_result {
                            self.playlist.borrow_mut().select_index(index);
                        }
                        // Exit search mode
                        self.layers.pop();
                    },
                    KeyboardEvent::Delete => {
                        // Gérer la touche Delete avec le même pattern
                        {
                            search.borrow_mut().handle_delete();
                        } // The mutable borrow ends here
                        
                        // Copier la requête
                        let query = search.borrow().input().to_string();
                        
                        // Chercher les éléments correspondants
                        let index = if !query.is_empty() {
                            self.playlist.borrow().search(&query)
                        } else {
                            None
                        };
                        
                        // Mettre à jour le résultat de recherche
                        {
                            search.borrow_mut().set_search_result(index);
                        }
                    },
                    KeyboardEvent::Left => {
                        search.borrow_mut().move_cursor_left();
                    },
                    KeyboardEvent::Right => {
                        search.borrow_mut().move_cursor_right();
                    },
                    // Tous les autres événements clavier sont délibérément ignorés quand le widget de recherche est actif
                    // pour éviter que les actions de la playlist ou d'autres widgets ne soient déclenchées
                    _ => {}
                }
            },
            // Handle other screens as before
            Screens::OutputSelector(selector) => {
                match event {
                    KeyboardEvent::Quit => {
                        self.layers.pop();
                    },
                    KeyboardEvent::Up => selector.borrow_mut().select_previous(),
                    KeyboardEvent::Down => selector.borrow_mut().select_next(),
                    KeyboardEvent::Enter => {
                        selector.borrow_mut().set_selected_device()?;
                        self.layers.pop();
                    },
                    KeyboardEvent::Escape => {
                        self.layers.pop();
                    },
                    _ => {}
                }
            },
            Screens::Default(playlist) => {
                match event {
                    KeyboardEvent::Quit => {
                        playlist.borrow_mut().stop().await?;
                        return Ok(());
                    },
                    KeyboardEvent::Search => {
                        // Activate search widget when '/' is pressed
                        self.search_widget.borrow_mut().clear();
                        self.layers.push(Screens::SearchWidget(self.search_widget.clone()));
                    },
                    KeyboardEvent::DeviceSelector => {
                        self.output_selector.borrow_mut().refresh_device_list()?;
                        self.layers.push(Screens::OutputSelector(self.output_selector.clone()));
                    },
                    KeyboardEvent::Play | KeyboardEvent::Pause => {
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
