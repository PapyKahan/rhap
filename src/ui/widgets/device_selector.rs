use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::{Alignment, Constraint, Rect},
    widgets::{Block, Borders, Cell, Clear, Row, Table, TableState},
    Frame,
};

use crate::{
    action::Action,
    audio::{Device, DeviceTrait, Host, HostTrait},
    ui::component::{Component, RenderContext},
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
            if !self
                .devices
                .iter()
                .any(|item| -> bool { &item.name().unwrap_or_default() == device })
            {
                self.selected = None;
            }
        }

        Ok(())
    }

    fn set_selected_device(&mut self) -> Result<()> {
        self.selected = match self.state.selected() {
            Some(i) => Some(if i < self.devices.len() {
                self.devices[i].name()?
            } else {
                self.default.name()?
            }),
            None => None,
        };
        Ok(())
    }

    fn select_previous(&mut self) {
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

    fn select_next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i + 1 >= self.devices.len() {
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

impl Component for DeviceSelector {
    fn render(&mut self, frame: &mut Frame, area: Rect, ctx: &RenderContext) -> Result<()> {
        let default = &self.default.name()?.clone();
        let selected_device_name = if let Some(device) = self.selected.as_ref() {
            device
        } else {
            default
        };

        let mut items = Vec::new();
        for device in &self.devices {
            let is_selected = &device.name()? == selected_device_name;
            let row = Row::new(vec![
                Cell::from(if is_selected { "󰓃" } else { "  " }),
                Cell::from(device.name()?),
            ])
            .height(1)
            .style(if items.len() % 2 == 0 {
                ctx.theme.table.row_even
            } else {
                ctx.theme.table.row_odd
            });
            items.push(row);
        }

        let table = Table::new(items, &[Constraint::Length(1), Constraint::Percentage(100)])
            .highlight_symbol("=>")
            .row_highlight_style(ctx.theme.table.highlight)
            .block(
                Block::default()
                    .title("Select Output Device")
                    .title_alignment(Alignment::Center)
                    .borders(Borders::ALL)
                    .border_type(ratatui::widgets::BorderType::Rounded)
                    .border_style(ctx.theme.border),
            );

        frame.render_widget(Clear, area);
        frame.render_stateful_widget(table, area, &mut self.state);
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Action> {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Ok(Action::PopLayer),
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                Ok(Action::None)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                Ok(Action::None)
            }
            KeyCode::Enter => {
                self.set_selected_device()?;
                let index = self.state.selected().unwrap_or(0) as u32;
                Ok(Action::Batch(vec![
                    Action::ChangeOutputDevice(index),
                    Action::PopLayer,
                ]))
            }
            _ => Ok(Action::None),
        }
    }
}
