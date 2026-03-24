use super::{
    component::Component,
    screens::Playlist,
    utils::bottom_right_fixed_size,
    widgets::{DeviceSelector, SearchWidget},
};
use crate::{
    action::{Action, Layer},
    app_state::AppState,
    audio::Host,
    media_controls::MediaControlsBackend,
    player::Player,
    ui::theme::Theme,
};
#[cfg(target_os = "windows")]
use crate::notifications::NotificationsBackend;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::SetTitle;
use crossterm::ExecutableCommand;
use log::error;
use ratatui::DefaultTerminal;
use std::path::PathBuf;
use std::time::Duration;

pub struct App {
    state: AppState,
    theme: Theme,
    playlist: Playlist,
    output_selector: DeviceSelector,
    search_widget: SearchWidget,
    media_event_rx: Option<std::sync::mpsc::Receiver<Action>>,
}

impl App {
    pub fn new(
        host: Host,
        player: Player,
        path: PathBuf,
        media_controls: Option<MediaControlsBackend>,
        media_event_rx: Option<std::sync::mpsc::Receiver<Action>>,
        #[cfg(target_os = "windows")]
        notifications: Option<NotificationsBackend>,
        #[cfg(not(target_os = "windows"))]
        _notifications: Option<std::convert::Infallible>,
        picker: Option<ratatui_image::picker::Picker>,
    ) -> Result<Self> {
        Ok(Self {
            #[cfg(target_os = "windows")]
            state: AppState::new(player, media_controls, notifications),
            #[cfg(not(target_os = "windows"))]
            state: AppState::new(player, media_controls, None),
            theme: Theme::default(),
            playlist: Playlist::new(path, picker)?,
            output_selector: DeviceSelector::new(host)?,
            search_widget: SearchWidget::new(),
            media_event_rx,
        })
    }

    fn dispatch_event(&mut self, key: crossterm::event::KeyEvent) -> Result<Action> {
        match self.state.layers.last().copied() {
            Some(Layer::Search) => self.search_widget.handle_key_event(key),
            Some(Layer::OutputSelector) => self.output_selector.handle_key_event(key),
            None => {
                // Handle Ctrl+N/P with search context before routing to playlist
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('n') => {
                            return Ok(Action::SearchNext(
                                self.search_widget.last_query().to_string(),
                            ));
                        }
                        KeyCode::Char('p') => {
                            return Ok(Action::SearchPrev(
                                self.search_widget.last_query().to_string(),
                            ));
                        }
                        _ => {}
                    }
                }
                self.playlist.handle_key_event(key)
            }
        }
    }

    fn process_action(&mut self, action: Action) -> Result<()> {
        self.state.process_action(
            action,
            &mut self.playlist,
            &mut self.search_widget,
            &mut self.output_selector,
        )
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        terminal
            .backend_mut()
            .execute(SetTitle("rhap - Rust Handcrafted Audio Player"))?;

        loop {
            // 1. Drain all pending input (zero-wait)
            while event::poll(Duration::ZERO)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    // Ctrl+C → quit
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        self.process_action(Action::Quit)?;
                        return Ok(());
                    }
                    let action = self.dispatch_event(key)?;
                    if matches!(action, Action::Quit) {
                        self.process_action(action)?;
                        return Ok(());
                    }
                    self.process_action(action)?;
                }
            }

            // 2. Drain media control events
            {
                let mut media_actions = vec![];
                if let Some(rx) = &self.media_event_rx {
                    while let Ok(action) = rx.try_recv() {
                        media_actions.push(action);
                    }
                }
                for action in media_actions {
                    self.process_action(action)?;
                }
            }

            // 3. Sync media controls (pumps Windows messages + updates OS overlay)
            self.state.sync_media_controls();

            // 4. Auto-advance
            self.state.auto_advance(&self.playlist)?;

            // 5. Render
            let ctx = self.state.render_context(&self.theme);
            terminal.draw(|frame| {
                self.playlist
                    .render(frame, frame.area(), &ctx)
                    .unwrap_or_else(|err| {
                        error!("error while drawing playlist: {}", err);
                    });
                match self.state.layers.last().copied() {
                    Some(Layer::OutputSelector) => {
                        let area = bottom_right_fixed_size(40, 6, frame.area());
                        self.output_selector
                            .render(frame, area, &ctx)
                            .unwrap_or_else(|err| {
                                error!("error while drawing output selector: {}", err);
                            });
                    }
                    Some(Layer::Search) => {
                        self.search_widget
                            .render(frame, frame.area(), &ctx)
                            .unwrap_or_else(|err| {
                                error!("error while drawing search widget: {}", err);
                            });
                    }
                    None => {}
                }
            })?;

            // 6. Frame limiter: sleep up to 16ms, wake on input
            let _ = event::poll(Duration::from_millis(16));
        }
    }
}
