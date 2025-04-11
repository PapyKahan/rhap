use super::{
    screens::Playlist,
    utils::bottom_right_fixed_size,
    widgets::{DeviceSelector, SearchWidget},
};
use super::{KeyboardEvent, KeyboardManager};
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

    // Helper function to exit search mode
    fn exit_search_mode(&mut self) {
        self.keyboard_manager.set_search_mode(false);
        self.layers.pop();
    }

    async fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> Result<()> {
        let default_screen = Screens::Default(self.playlist.clone());
        let current_screen = self.layers.last().unwrap_or(&default_screen);

        match current_screen {
            Screens::SearchWidget(search) => {
                match event {
                    KeyboardEvent::Escape => {
                        self.exit_search_mode();
                    }
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
                    }
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
                    }
                    KeyboardEvent::Enter => {
                        // Same approach - get data first, then perform actions
                        let search_result = search.borrow().search_result();
                        if let Some(index) = search_result {
                            self.playlist.borrow_mut().select_index(index);
                        }
                        self.exit_search_mode();
                    }
                    KeyboardEvent::Delete => {
                        // Handle Delete key with the same pattern
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
                    }
                    KeyboardEvent::Left => {
                        search.borrow_mut().move_cursor_left();
                    }
                    KeyboardEvent::Right => {
                        search.borrow_mut().move_cursor_right();
                    }
                    // All other keyboard events are deliberately ignored when the search widget is active
                    // to prevent actions from the playlist or other widgets from being triggered
                    _ => {}
                }
            }
            // Handle other screens as before
            Screens::OutputSelector(selector) => match event {
                KeyboardEvent::Quit => {
                    self.layers.pop();
                }
                KeyboardEvent::Up => selector.borrow_mut().select_previous(),
                KeyboardEvent::Down => selector.borrow_mut().select_next(),
                KeyboardEvent::Enter => {
                    selector.borrow_mut().set_selected_device()?;
                    self.layers.pop();
                }
                KeyboardEvent::Escape => {
                    self.layers.pop();
                }
                _ => {}
            },
            Screens::Default(playlist) => {
                match event {
                    KeyboardEvent::Quit => {
                        playlist.borrow_mut().stop().await?;
                        return Ok(());
                    }
                    KeyboardEvent::Search => {
                        // Activate the search widget
                        self.search_widget.borrow_mut().clear();
                        self.keyboard_manager.set_search_mode(true); // Enable search mode
                        self.layers
                            .push(Screens::SearchWidget(self.search_widget.clone()));
                    }
                    KeyboardEvent::DeviceSelector => {
                        self.output_selector.borrow_mut().refresh_device_list()?;
                        self.layers
                            .push(Screens::OutputSelector(self.output_selector.clone()));
                    }
                    KeyboardEvent::Play | KeyboardEvent::Pause => {
                        if playlist.borrow_mut().is_playing() {
                            playlist.borrow_mut().pause().await?;
                        } else {
                            playlist.borrow_mut().resume().await?;
                        }
                    }
                    KeyboardEvent::Stop => playlist.borrow_mut().stop().await?,
                    KeyboardEvent::Next => playlist.borrow_mut().next().await?,
                    KeyboardEvent::Previous => playlist.borrow_mut().previous().await?,
                    KeyboardEvent::Up => playlist.borrow_mut().select_previous(),
                    KeyboardEvent::Down => playlist.borrow_mut().select_next(),
                    KeyboardEvent::Enter => playlist.borrow_mut().play_selected().await?,
                    KeyboardEvent::NextMatch => {
                        // Get the last search query from search widget
                        let query = self.search_widget.borrow().last_query().to_string();

                        if !query.is_empty() {
                            // Get current selected index as the starting point
                            let current_index = playlist.borrow().selected_index();

                            // Find the next match
                            let next_match = playlist.borrow().search_next(current_index, &query);

                            // If found, select that item
                            if let Some(index) = next_match {
                                playlist.borrow_mut().select_index(index);
                            }
                        }
                    }
                    KeyboardEvent::PrevMatch => {
                        // Get the last search query from search widget
                        let query = self.search_widget.borrow().last_query().to_string();

                        if !query.is_empty() {
                            // Get current selected index as the starting point
                            let current_index = playlist.borrow().selected_index();

                            // Find the previous match
                            let prev_match = playlist.borrow().search_prev(current_index, &query);

                            // If found, select that item
                            if let Some(index) = prev_match {
                                playlist.borrow_mut().select_index(index);
                            }
                        }
                    }
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

            // Keyboard event handling
            if event::poll(std::time::Duration::from_millis(100))? {
                if let Ok(event) = event::read() {
                    self.keyboard_manager.handle_event(event).await?;
                }
            }

            // Processing keyboard events
            while let Ok(event) = self.event_receiver.try_recv() {
                self.handle_keyboard_event(&event).await?;
                if let KeyboardEvent::Quit = event {
                    return Ok(());
                }
            }

            // Update the interface
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
