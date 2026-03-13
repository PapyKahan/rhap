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
use std::path::PathBuf;
use tokio::sync::broadcast;

#[derive(Clone, Copy)]
enum Layer {
    OutputSelector,
    Search,
}

pub struct App {
    layers: Vec<Layer>,
    output_selector: DeviceSelector,
    search_widget: SearchWidget,
    playlist: Playlist,
    keyboard_manager: KeyboardManager,
    event_receiver: broadcast::Receiver<KeyboardEvent>,
}

impl App {
    pub fn new(host: Host, player: Player, path: PathBuf) -> Result<Self> {
        let keyboard_manager = KeyboardManager::new();
        let event_receiver = keyboard_manager.get_receiver();

        Ok(Self {
            layers: vec![],
            output_selector: DeviceSelector::new(host)?,
            search_widget: SearchWidget::new(),
            playlist: Playlist::new(path, player)?,
            keyboard_manager,
            event_receiver,
        })
    }

    fn render(&mut self, frame: &mut Frame) -> Result<()> {
        self.playlist.render(frame, frame.area())?;
        match self.layers.last().copied() {
            Some(Layer::OutputSelector) => {
                let area = bottom_right_fixed_size(40, 6, frame.area());
                self.output_selector.render(frame, area)?;
            }
            Some(Layer::Search) => {
                self.search_widget.render(frame, frame.area());
            }
            None => {}
        }
        Ok(())
    }

    // Helper function to exit search mode
    fn exit_search_mode(&mut self) {
        self.keyboard_manager.set_search_mode(false);
        self.layers.pop();
    }

    async fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> Result<()> {
        match self.layers.last().copied() {
            Some(Layer::Search) => {
                match event {
                    KeyboardEvent::Escape => {
                        self.exit_search_mode();
                    }
                    KeyboardEvent::Backspace => {
                        self.search_widget.handle_backspace();
                        let query = self.search_widget.input().to_string();
                        let index = if !query.is_empty() {
                            self.playlist.search(&query)
                        } else {
                            None
                        };
                        self.search_widget.set_search_result(index);
                    }
                    KeyboardEvent::Char(c) => {
                        self.search_widget.handle_input(*c);
                        let query = self.search_widget.input().to_string();
                        let index = self.playlist.search(&query);
                        self.search_widget.set_search_result(index);
                    }
                    KeyboardEvent::Enter => {
                        if let Some(index) = self.search_widget.search_result() {
                            self.playlist.select_index(index);
                        }
                        self.exit_search_mode();
                    }
                    KeyboardEvent::Delete => {
                        self.search_widget.handle_delete();
                        let query = self.search_widget.input().to_string();
                        let index = if !query.is_empty() {
                            self.playlist.search(&query)
                        } else {
                            None
                        };
                        self.search_widget.set_search_result(index);
                    }
                    KeyboardEvent::Left => {
                        self.search_widget.move_cursor_left();
                    }
                    KeyboardEvent::Right => {
                        self.search_widget.move_cursor_right();
                    }
                    _ => {}
                }
            }
            Some(Layer::OutputSelector) => match event {
                KeyboardEvent::Quit => {
                    self.layers.pop();
                }
                KeyboardEvent::Up => self.output_selector.select_previous(),
                KeyboardEvent::Down => self.output_selector.select_next(),
                KeyboardEvent::Enter => {
                    self.output_selector.set_selected_device()?;
                    self.layers.pop();
                }
                KeyboardEvent::Escape => {
                    self.layers.pop();
                }
                _ => {}
            },
            None => {
                match event {
                    KeyboardEvent::Quit => {
                        self.playlist.stop()?;
                        return Ok(());
                    }
                    KeyboardEvent::Search => {
                        self.search_widget.clear();
                        self.keyboard_manager.set_search_mode(true);
                        self.layers.push(Layer::Search);
                    }
                    KeyboardEvent::DeviceSelector => {
                        self.output_selector.refresh_device_list()?;
                        self.layers.push(Layer::OutputSelector);
                    }
                    KeyboardEvent::Play | KeyboardEvent::Pause => {
                        if self.playlist.is_playing() {
                            self.playlist.pause()?;
                        } else {
                            self.playlist.resume()?;
                        }
                    }
                    KeyboardEvent::Stop => self.playlist.stop()?,
                    KeyboardEvent::Next => self.playlist.next()?,
                    KeyboardEvent::Previous => self.playlist.previous()?,
                    KeyboardEvent::Up => self.playlist.select_previous(),
                    KeyboardEvent::Down => self.playlist.select_next(),
                    KeyboardEvent::Enter => self.playlist.play_selected()?,
                    KeyboardEvent::NextMatch => {
                        let query = self.search_widget.last_query().to_string();
                        if !query.is_empty() {
                            let current_index = self.playlist.selected_index();
                            if let Some(index) = self.playlist.search_next(current_index, &query) {
                                self.playlist.select_index(index);
                            }
                        }
                    }
                    KeyboardEvent::PrevMatch => {
                        let query = self.search_widget.last_query().to_string();
                        if !query.is_empty() {
                            let current_index = self.playlist.selected_index();
                            if let Some(index) = self.playlist.search_prev(current_index, &query) {
                                self.playlist.select_index(index);
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
            if self.layers.is_empty() {
                self.playlist.run()?;
            }
        }
    }
}
