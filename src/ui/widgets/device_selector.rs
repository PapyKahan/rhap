use ratatui::{widgets::{TableState, Row, Cell, Table, Block, Borders}, style::{Style, Color}, prelude::{Constraint, Alignment}};
use anyhow::{Result, anyhow};
use crate::audio::{Device, HostTrait, DeviceTrait, create_host};


pub struct DeviceSelector<'devicelist> {
    pub show_popup: bool,
    selected: Device,
    pub state: TableState,
    devices: Vec<Row<'devicelist>>,
}

impl<'devicelist> DeviceSelector<'devicelist> {
    pub fn new() -> Result<DeviceSelector<'devicelist>> {
        let host = create_host("wasapi");
        let devices = host.get_devices().map_err(|err| anyhow!(err.to_string()))?;
        let mut index = 0;
        let mut items = Vec::new();
        let mut state = TableState::default();
        state.select(Some(0));
        for dev in devices {
            if dev.is_default() {
                state.select(Some(index));
            }
            let row = Row::new(vec![
                Cell::from(if dev.is_default() { "*" } else { "  " }),
                Cell::from(index.to_string()),
                Cell::from(dev.name()),
            ])
            .height(1)
            .style(Style::default().fg(Color::White));
            items.push(row);
            index = index + 1;
        }
        Ok(DeviceSelector {
            show_popup: false,
            state,
            selected: Device::None,
            devices: items,
        })
    }

    pub fn set_selected_device(&mut self) -> Result<()> {
        if self.show_popup {
            let host = create_host("wasapi");
            let devices = host.get_devices().map_err(|err| anyhow!(err.to_string()))?;
            self.selected = match self.state.selected() {
                Some(i) => devices[i].clone(),
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

        let host = create_host("wasapi");
        let devices = host.get_devices().map_err(|err| anyhow!(err.to_string()))?;
        let mut index = 0;
        let mut items = Vec::new();

        let selected_device = match self.selected {
            Device::None => host.get_default_device().map_err(|err| anyhow!(err.to_string()))?,
            _ => self.selected.clone()
        };

        for dev in devices {
            let is_selected = dev.name() == selected_device.name();
            let row = Row::new(vec![
                Cell::from(if is_selected { "󰓃" } else { "  " }),
                Cell::from(dev.name()),
            ])
            .height(1)
            .style(Style::default().bg(if index % 2 == 0 { Color::Rgb(80, 80, 80) } else { Color::Rgb(50, 50, 50) }));
            items.push(row);
            index = index + 1;
        }

        let table = Table::new(items)
            .highlight_symbol("=>")
            .highlight_style(Style::default().fg(color))
            .widths(&[
                Constraint::Length(1),
                Constraint::Percentage(100),
            ])
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
}
