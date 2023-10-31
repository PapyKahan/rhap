use crate::{
    audio::{Device, DeviceTrait, Host, HostTrait},
    ui::{HIGHLIGHT_COLOR, ROW_ALTERNATE_COLOR, ROW_COLOR},
};
use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    prelude::{Alignment, Backend, Constraint, Rect},
    style::Style,
    widgets::{Block, Borders, Cell, Clear, Row, Table, TableState},
    Frame,
};

pub struct DeviceSelector {
    state: TableState,
    host: Host,
    selected: Option<String>,
    default: Device,
    devices: Vec<Device>,
}

impl DeviceSelector {
    pub fn new(host: Host) -> Result<DeviceSelector> {
        let mut state = TableState::default();
        state.select(Some(0));

        Ok(DeviceSelector {
            state,
            host,
            selected: None,
            default: Device::None,
            devices: Vec::new(),
        })
    }

    pub fn refresh_device_list(&mut self) -> Result<()> {
        self.devices = self
            .host
            .get_devices()
            .map_err(|err| anyhow!(err.to_string()))?;
        self.default = self
            .host
            .get_default_device()
            .map_err(|err| anyhow!(err.to_string()))?;
        self.state.select(Some(0));

        if let Some(device) = self.selected.as_ref() {
            if !self.devices.iter().any(|item| &item.name() == device) {
                self.selected = None;
            }
        }

        Ok(())
    }

    pub fn set_selected_device(&mut self) -> Result<()> {
        self.selected = match self.state.selected() {
            Some(i) => Some(if i < self.devices.len() {
                self.devices[i].name()
            } else {
                self.default.name()
            }),
            None => None,
        };
        Ok(())
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.devices.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.devices.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn event_hanlder(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Up => self.previous(),
                KeyCode::Down => self.next(),
                KeyCode::Enter => self.set_selected_device()?,
                _ => (),
            }
        }
        Ok(())
    }

    pub(crate) fn render<B: Backend>(&mut self, frame: &mut Frame<B>, area: Rect) -> Result<()> {
        let default = &self.default.name().clone();
        let selected_device_name = if let Some(device) = self.selected.as_ref() {
            device
        } else {
            default
        };

        let mut items = Vec::new();
        for device in &self.devices {
            let is_selected = &device.name() == selected_device_name;
            let row = Row::new(vec![
                Cell::from(if is_selected { "󰓃" } else { "  " }),
                Cell::from(device.name()),
            ])
            .height(1)
            .style(Style::default().bg(if items.len() % 2 == 0 {
                ROW_COLOR
            } else {
                ROW_ALTERNATE_COLOR
            }));
            items.push(row);
        }

        let table = Table::new(items)
            .highlight_symbol("=>")
            .highlight_style(Style::default().fg(HIGHLIGHT_COLOR))
            .widths(&[Constraint::Length(1), Constraint::Percentage(100)])
            .block(
                Block::default()
                    .title("Select Output Device")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::default().fg(HIGHLIGHT_COLOR)),
            );

        frame.render_widget(Clear, area);
        frame.render_stateful_widget(table, area, &mut self.state);
        Ok(())
    }
}
