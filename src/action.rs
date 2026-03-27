/// Identifies the active layer/modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    OutputSelector,
    Search,
}

/// Actions emitted by UI frontends, processed by AppState.
/// No ratatui/crossterm types — this is the shared API.
/// Future: #[derive(Serialize, Deserialize)] for web transport.
pub enum Action {
    None,
    // Playback
    TogglePlayPause,
    Stop,
    NextTrack,
    PreviousTrack,
    PlaySelected,
    ChangeOutputDevice(u32),
    // Navigation
    SelectUp,
    SelectDown,
    // Layers
    PushLayer(Layer),
    PopLayer,
    // Search
    SearchQuery(String),
    SearchNext(String),
    SearchPrev(String),
    CommitSearch(Option<usize>),
    // App
    Quit,
    // Batch multiple actions
    Batch(Vec<Action>),
}
