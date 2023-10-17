use std::{cell::RefCell, rc::Rc};

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
    OutputSelector(Rc<RefCell<DeviceSelector>>),
}

pub struct App {
    layers: Vec<Screens>,
    output_selector: Rc<RefCell<DeviceSelector>>,
}

impl App {
    pub fn new(host: Host) -> Result<Self> {
        Ok(Self {
            layers: vec![],
            output_selector: Rc::new(RefCell::new(DeviceSelector::new(host)?)),
        })
    }

    fn ui<B: Backend>(&mut self, frame: &mut Frame<B>) -> Result<()> {
        let size = frame.size();

        let block = Block::default().title("Content").borders(Borders::ALL);
        frame.render_widget(block, size);

        let layer = self.layers.last().unwrap_or(&Screens::None);
        match layer {
            Screens::OutputSelector(selector) => {
                let area = Self::centered_rect(20, 10, size);
                (*selector).borrow_mut().render(frame, area)?;
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

            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    let screen = self.layers.last().unwrap_or(&Screens::None);
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
                        Screens::None => {
                            if key.kind == event::KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Char('q') => return Ok(()),
                                    KeyCode::Char('p') => {
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
