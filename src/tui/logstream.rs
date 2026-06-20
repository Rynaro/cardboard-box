//! Log streaming helpers: `LogCoalescer` (batching) and related constants.
//!
//! This module is LEAN — no ratatui dep, no tui feature gate. The coalescer is
//! pure (time-injectable for deterministic tests).
#![allow(dead_code)]

// ─── Constants ────────────────────────────────────────────────────────────────

/// Number of lines that trigger a coalescer flush (size-based trigger).
pub const FLUSH_LINES: usize = 64;

/// Elapsed milliseconds before a partial batch is flushed (time-based trigger).
pub const FLUSH_INTERVAL_MS: u64 = 50;

/// Maximum number of log lines kept in the bounded ring buffer on the Model.
/// Oldest lines are dropped silently when the cap is exceeded (drop-oldest).
pub const LOG_RING_CAP: usize = 2000;

/// Scroll step (lines per scroll-wheel notch) for the log pane and command-log.
pub const SCROLL_STEP: usize = 3;

// ─── LogCoalescer ─────────────────────────────────────────────────────────────

/// Batches incoming log lines for bulk delivery as `Message::LogChunk`.
///
/// Flush triggers:
///   - Size: `push` returns `true` when the buffer reaches `FLUSH_LINES`.
///   - Time: `due_at(elapsed_ms)` returns `true` when `elapsed_ms >= FLUSH_INTERVAL_MS`
///     AND the buffer is non-empty. The caller drives the clock by passing the
///     milliseconds elapsed since the last flush — this makes the coalescer
///     deterministically unit-testable without a real `Instant`.
pub struct LogCoalescer {
    buf: Vec<String>,
}

impl LogCoalescer {
    pub fn new() -> Self {
        LogCoalescer { buf: Vec::new() }
    }

    /// Push a line and return `true` when the size-based flush trigger fires.
    pub fn push(&mut self, line: String) -> bool {
        self.buf.push(line);
        self.buf.len() >= FLUSH_LINES
    }

    /// Check the time-based flush trigger.
    ///
    /// `elapsed_ms` — milliseconds since the last flush (caller computes this
    /// from e.g. `Instant::now() - last_flush_at`).
    ///
    /// Returns `true` when `elapsed_ms >= FLUSH_INTERVAL_MS` and the buffer is
    /// non-empty.
    pub fn due_at(&self, elapsed_ms: u64) -> bool {
        !self.buf.is_empty() && elapsed_ms >= FLUSH_INTERVAL_MS
    }

    /// Drain and return all buffered lines; empties the buffer.
    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.buf)
    }

    /// Whether the buffer is currently empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl Default for LogCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

// ─── LogBuffer ────────────────────────────────────────────────────────────────

/// Bounded ring buffer of log lines on the Model.
///
/// Mirrors `CmdLog` (cmdlog.rs:34-75) and `StatsHistory.push_sample`
/// (model.rs:236-250): VecDeque-with-cap, drop-oldest past cap.
///
/// This is a PURE type (no I/O); it lives on the Model.
pub struct LogBuffer {
    lines: std::collections::VecDeque<String>,
    cap: usize,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        LogBuffer {
            lines: std::collections::VecDeque::new(),
            cap,
        }
    }

    /// Push a chunk of lines, dropping the oldest past cap.
    pub fn push_chunk(&mut self, chunk: Vec<String>) {
        for line in chunk {
            self.lines.push_back(line);
            while self.lines.len() > self.cap {
                self.lines.pop_front();
            }
        }
    }

    /// Iterate over all lines, oldest first (newest at bottom for autoscroll).
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|s| s.as_str())
    }

    /// Number of lines currently in the buffer.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Clear the buffer (used on target swap).
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}
