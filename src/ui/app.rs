use anyhow::Result;
use ratatui::{prelude::{Backend, Layout, Direction, Constraint, Rect}, Frame, widgets::{Block, Borders}};
use crate::audio::Host;
use super::widgets::DeviceSelector;


pub enum Screens {
    None,
    OutputSelector(DeviceSelector),
}

pub struct App {
    pub host: Host,
    pub screens: Vec<Screens>,
}

impl App {
    pub fn new(host: Host) -> Result<Self> {
        Ok(Self {
            host,
            screens: vec![],
        })
    }

    pub fn ui<B: Backend>(&mut self, frame: &mut Frame<B>) -> Result<()> {
        let size = frame.size();

        let block = Block::default().title("Content").borders(Borders::ALL);
        frame.render_widget(block, size);

        let screen = self.screens.pop().unwrap_or(Screens::None);
        match screen {
            Screens::OutputSelector(mut selector) => {
                let area = Self::centered_rect(20, 10, size);
                selector.render(frame, area)?;
                self.screens.push(Screens::OutputSelector(selector));
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
}
