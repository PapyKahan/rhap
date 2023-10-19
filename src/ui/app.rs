use super::{screens::Playlist, widgets::DeviceSelector};
use crate::{audio::Host, player::Player};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    prelude::{Backend, Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame, Terminal,
};
use std::{cell::RefCell, rc::Rc, path::PathBuf};

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
            playlist: Rc::new(RefCell::new(Playlist::new(path))),
        })
    }

    fn render<B: Backend>(&mut self, frame: &mut Frame<B>) -> Result<()> {
        self.playlist.borrow_mut().render(frame, frame.size())?;
        let default = Screens::Default(self.playlist.clone());
        let layer = self.layers.last().unwrap_or(&default);
        match layer {
            Screens::OutputSelector(selector) => {
                let area = Self::bottom_right_fixed_size(40, 6, frame.size());
                (*selector).borrow_mut().render(frame, area)?;
            },
            _ => ()

        }
        Ok(())
    }

    /// helper function to create a centered rect using up certain percentage of the available rect `r`
    fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }

    fn bottom_right_fixed_size(width: u16, height: u16, area: Rect) -> Rect {
        let col = area.width - width;
        let row = area.height - height;
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(row), Constraint::Length(height)])
            .split(area);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(col), Constraint::Length(width)])
            .split(popup_layout[1])[1]
    }

    /// helper function to create a centered rect using up certain percentage of the available rect `r`
    fn centered_fixed_size_rect(width: u16, height: u16, area: Rect) -> Rect {
        let col = (area.width - width) / 2;
        let row = (area.height - height) / 2;
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(row),
                Constraint::Length(height),
                Constraint::Length(row),
            ])
            .split(area);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(col),
                Constraint::Length(width),
                Constraint::Length(col),
            ])
            .split(popup_layout[1])[1]
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
                    let screen = self
                        .layers
                        .last()
                        .unwrap_or(&default);
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
