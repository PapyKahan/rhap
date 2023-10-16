use crate::audio::{Device, DeviceTrait, Host, HostTrait};
use anyhow::{anyhow, Result};
use crossterm::event::{Event, KeyEvent, KeyEventKind, KeyCode};
use ratatui::{
    prelude::{Alignment, Constraint},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

pub struct DeviceSelector {
    pub show_popup: bool,
    pub state: TableState,
    selected: Device,
    default: Device,
    devices: Vec<Device>,
}

impl<'deviceselector> DeviceSelector {
    pub fn new(host: Host) -> Result<DeviceSelector> {
        let devices = host.get_devices().map_err(|err| anyhow!(err.to_string()))?;
        let default = host
            .get_default_device()
            .map_err(|err| anyhow!(err.to_string()))?;
        let mut state = TableState::default();
        state.select(Some(0));

        Ok(DeviceSelector {
            show_popup: false,
            state,
            selected: Device::None,
            default,
            devices,
        })
    }

    pub fn set_selected_device(&mut self) -> Result<()> {
        if self.show_popup {
            self.selected = match self.state.selected() {
                Some(i) => {
                    if i < self.devices.len() - 1 {
                        self.devices[i].clone()
                    } else {
                        self.default.clone()
                    }
                }
                None => Device::None,
            };
        }
        Ok(())
    }

    pub fn next(&mut self) {
        if self.show_popup {
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
    }

    pub fn previous(&mut self) {
        if self.show_popup {
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
    }

    pub fn ui(&self) -> Result<Table> {
        let color = Color::Rgb(255, 191, 0);
        let selected_device = match self.selected {
            Device::None => &self.default,
            _ => &self.selected,
        };

        let mut items = Vec::new();
        for device in &self.devices {
            let is_selected = device.name() == selected_device.name();
            let row = Row::new(vec![
                Cell::from(if is_selected { "󰓃" } else { "  " }),
                Cell::from(device.name()),
            ])
            .height(1)
            .style(Style::default().bg(if items.len() % 2 == 0 {
                Color::Rgb(80, 80, 80)
            } else {
                Color::Rgb(50, 50, 50)
            }));
            items.push(row);
        }

        let table = Table::new(items)
            .highlight_symbol("=>")
            .highlight_style(Style::default().fg(color))
            .widths(&[Constraint::Length(1), Constraint::Percentage(100)])
            .block(
                Block::default()
                    .title("Select Output Device")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(Style::default().fg(color)),
            );
        Ok(table)
    }

    pub fn event_hanlder(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Up => self.previous(),
                KeyCode::Down => self.next(),
                KeyCode::Enter => self.set_selected_device()?,
                _ => ()
            }
        }
        Ok(())
    }
}
