use anyhow::Result;
use log::error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Handle for a backend audio output thread that pulls samples from a
/// ring buffer and writes them to the device (WASAPI, ALSA — but not
/// PipeWire, which is callback-driven).
///
/// Owns the thread join handle and the two atomic flags the audio loop
/// observes:
/// - `is_playing`: cleared to make the loop break and exit.
/// - `is_paused`: toggled to gate writes (the audio thread is responsible
///   for translating this to backend-specific pause semantics).
pub struct PullStreamHandle {
    thread: Option<JoinHandle<Result<()>>>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
}

impl PullStreamHandle {
    /// Construct a handle from a freshly spawned thread and shared flags.
    /// Caller is expected to keep clones of the same flags inside the thread.
    pub fn new(
        thread: JoinHandle<Result<()>>,
        is_playing: Arc<AtomicBool>,
        is_paused: Arc<AtomicBool>,
    ) -> Self {
        Self {
            thread: Some(thread),
            is_playing,
            is_paused,
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::Release);
    }

    /// Signal the audio thread to exit and join it. Errors from the thread
    /// (including panics) are logged but not propagated — `stop` is called
    /// from `Drop` and other cleanup paths where there's nothing useful to
    /// do with an error.
    pub fn stop(&mut self) {
        self.is_playing.store(false, Ordering::Release);
        // Clear pause too: if the thread is currently spinning in its
        // pause sub-loop, we want it to notice the !is_playing transition.
        self.is_paused.store(false, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            match handle.join() {
                Ok(Err(e)) => error!("Audio-out thread error: {:#}", e),
                Err(_) => error!("Audio-out thread panicked"),
                Ok(Ok(())) => {}
            }
        }
    }
}
