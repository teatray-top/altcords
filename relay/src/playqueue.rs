use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Shared playback state: the submitter enqueues, the TTS worker moves items
/// through `current`, the stop hotkey clears everything and bumps `epoch`, the
/// GUI sets `volume`, and the queue overlay renders it.
pub struct PlayQueue {
    inner: Mutex<Inner>,
    epoch: Arc<AtomicU64>,
    volume: Arc<AtomicU32>, // f32 bits, output gain
}

#[derive(Default)]
struct Inner {
    current: Option<String>,
    pending: VecDeque<String>,
}

impl PlayQueue {
    /// Create a shared queue with the given initial output volume.
    pub fn new(volume: f32) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            epoch: Arc::new(AtomicU64::new(0)),
            volume: Arc::new(AtomicU32::new(volume.to_bits())),
        })
    }

    /// Handle to the barge-in epoch (read by the player and the synth loop).
    pub fn epoch_handle(&self) -> Arc<AtomicU64> {
        self.epoch.clone()
    }

    /// Handle to the output-gain value (read by the player callback).
    pub fn volume_handle(&self) -> Arc<AtomicU32> {
        self.volume.clone()
    }

    /// Set the output gain (from the GUI slider).
    pub fn set_volume(&self, v: f32) {
        self.volume.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Current barge-in epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// True if a stop happened since `mine` was read.
    pub fn cancelled(&self, mine: u64) -> bool {
        self.epoch.load(Ordering::SeqCst) != mine
    }

    /// Queue a submitted message.
    pub fn enqueue(&self, text: String) {
        self.inner.lock().unwrap().pending.push_back(text);
    }

    /// Worker: mark a message as now playing. The worker consumes messages in the
    /// same FIFO order they were enqueued, so this one is always the queue head —
    /// pop it unconditionally (matching by text broke when the worker's trimmed
    /// copy differed from the enqueued raw text, leaving stale items to pile up).
    pub fn begin(&self, text: &str) {
        let mut i = self.inner.lock().unwrap();
        i.pending.pop_front();
        i.current = Some(text.to_string());
    }

    /// Worker: the current message finished.
    pub fn finish(&self) {
        self.inner.lock().unwrap().current = None;
    }

    /// Stop hotkey: abort the active utterance and drop everything queued.
    pub fn stop(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let mut i = self.inner.lock().unwrap();
        i.current = None;
        i.pending.clear();
    }

    /// Overlay text: the playing line plus any queued lines, or None when idle.
    pub fn overlay_text(&self) -> Option<String> {
        let i = self.inner.lock().unwrap();
        if i.current.is_none() && i.pending.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        if let Some(cur) = &i.current {
            lines.push(format!("▶  {cur}"));
        }
        for p in &i.pending {
            lines.push(format!("·  {p}"));
        }
        Some(lines.join("\n"))
    }
}

/// Show the queue in its own overlay window, or hide it when idle.
pub fn refresh_overlay(pq: &PlayQueue) {
    match pq.overlay_text() {
        Some(t) => {
            crate::queue_overlay::set_text(&t);
            crate::queue_overlay::show();
        }
        None => crate::queue_overlay::hide(),
    }
}
