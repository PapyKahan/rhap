use super::{screens::Playlist, utils::bottom_right_fixed_size, widgets::DeviceSelector};
use crate::{audio::Host, player::Player};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::SetTitle;
use crossterm::ExecutableCommand;
use log::error;
use ratatui::{DefaultTerminal, Frame};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

pub enum Screens {
    OutputSelector(Rc<RefCell<DeviceSelector>>),
    Default(Rc<RefCell<Playlist>>),
}

pub struct App {
    layers: Vec<Screens>,
    output_selector: Rc<RefCell<DeviceSelector>>,
    playlist: Rc<RefCell<Playlist>>,
}

impl App {
    pub fn new(host: Host, player: Player, path: PathBuf) -> Result<Self> {
        Ok(Self {
            layers: vec![],
            output_selector: Rc::new(RefCell::new(DeviceSelector::new(host)?)),
            playlist: Rc::new(RefCell::new(Playlist::new(path, player)?)),
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

    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal
            .backend_mut()
            .execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;
        let default = Screens::Default(self.playlist.clone());
        loop {
            terminal.draw(|frame| match self.render(frame) {
                Ok(ok) => ok,
                Err(err) => {
                    error!("error while drawing {}", err.to_string());
                    ()
                }
            })?;

            // handle crossterm events
            if event::poll(std::time::Duration::from_millis(100))? {
                let current_screen = self.layers.last().unwrap_or(&default);
                if let Event::Key(key) = event::read()? {
                    match current_screen {
                        Screens::OutputSelector(selector) => {
                            selector.borrow_mut().event_handler(key)?;
                            if key.kind == event::KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Char('q') => {
                                        self.layers.pop();
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Screens::Default(playlist) => {
                            playlist.borrow_mut().event_hanlder(key).await?;
                            if key.kind == event::KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Char('q') => {
                                        playlist.borrow_mut().stop().await?;
                                        return Ok(());
                                    }
                                    KeyCode::Char('o') => {
                                        self.output_selector.borrow_mut().refresh_device_list()?;
                                        self.layers.push(Screens::OutputSelector(
                                            self.output_selector.clone(),
                                        ));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }

            let current_screen = self.layers.last().unwrap_or(&default);
            match current_screen {
                Screens::Default(playlist) => {
                    playlist.borrow_mut().run().await?;
                }
                _ => {}
            }
        }
    }
}
