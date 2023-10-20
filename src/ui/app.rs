use super::{screens::Playlist, utils::bottom_right_fixed_size, widgets::DeviceSelector};
use crate::{audio::Host, player::Player};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{prelude::Backend, Frame, Terminal};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

pub enum Screens {
    OutputSelector(Rc<RefCell<DeviceSelector>>),
    Default(Rc<RefCell<Playlist>>),
}

pub struct App {
    layers: Vec<Screens>,
    player: Player,
    output_selector: Rc<RefCell<DeviceSelector>>,
    playlist: Rc<RefCell<Playlist>>,
}

impl App {
    pub fn new(host: Host, player: Player, path: PathBuf) -> Result<Self> {
        Ok(Self {
            layers: vec![],
            player,
            output_selector: Rc::new(RefCell::new(DeviceSelector::new(host)?)),
            playlist: Rc::new(RefCell::new(Playlist::new(path)?)),
        })
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>) -> Result<()> {
        self.playlist.borrow_mut().render(frame, frame.size())?;
        let layer = if self.layers.is_empty() {
            return Ok(());
        } else {
            self.layers.last().unwrap()
        };
        match layer {
            Screens::OutputSelector(selector) => {
                let area = bottom_right_fixed_size(40, 6, frame.size());
                (*selector).borrow_mut().render(frame, area)?;
            }
            _ => (),
        }
        Ok(())
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|frame| match self.render(frame) {
                Ok(ok) => ok,
                Err(err) => {
                    println!("error while drawing {}", err.to_string());
                    ()
                }
            })?;

            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    let default = Screens::Default(self.playlist.clone());
                    let screen = self.layers.last().unwrap_or(&default);
                    match screen {
                        Screens::OutputSelector(selector) => {
                            selector.borrow_mut().event_hanlder(key)?;
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
                            playlist.borrow_mut().event_hanlder(key)?;
                            if key.kind == event::KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Char('q') => return Ok(()),
                                    KeyCode::Char('p') => {
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
        }
    }
}
