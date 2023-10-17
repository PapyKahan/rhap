use std::{collections::HashSet, cell::RefCell};
use super::widgets::DeviceSelector;
use crate::audio::Host;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    prelude::{Backend, Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame, Terminal,
};

pub enum Screens {
    None,
    OutputSelector(DeviceSelector),
}

pub struct App {
    host: Host,
    layers: Vec<Screens>,
    screens: RefCell<HashSet<Screens>>,
}

impl App {
    pub fn new(host: Host) -> Result<Self> {
        Ok(Self {
            host,
            layers: vec![],
            screens: RefCell::new(HashSet::new())
        })
    }

    fn ui<B: Backend>(&mut self, frame: &mut Frame<B>) -> Result<()> {
        let size = frame.size();

        let block = Block::default().title("Content").borders(Borders::ALL);
        frame.render_widget(block, size);

        let layer = self.layers.pop().unwrap_or(Screens::None);
        match layer {
            Screens::OutputSelector(mut selector) => {
                let area = Self::centered_rect(20, 10, size);
                selector.render(frame, area)?;
                self.layers.push(Screens::OutputSelector(selector));
            }
            Screens::None => (),
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

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        loop {
            terminal.draw(|frame| match self.ui(frame) {
                Ok(ok) => ok,
                Err(err) => {
                    println!("error while drawing {}", err.to_string());
                    ()
                }
            })?;

            if event::poll(std::time::Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    let screen = self.layers.pop().unwrap_or(Screens::None);
                    match screen {
                        Screens::OutputSelector(mut selector) => {
                            selector.event_hanlder(key)?;
                            if key.kind == event::KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Char('q') => continue,
                                    _ => {}
                                }
                            }
                            self.layers.push(Screens::OutputSelector(selector));
                        }
                        Screens::None => {
                            if key.kind == event::KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Char('q') => return Ok(()),
                                    KeyCode::Char('p') => {
                                        let output_selector = Screens::OutputSelector(
                                            DeviceSelector::new(self.host)?,
                                        );
                                        self.layers.push(output_selector);
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
