//! Messages that the TUI event loop feeds into the pure reducer.

use crate::core::spec::{
    ApplyOutcome, BoxRow, DoctorResult, InspectResult, StatsSample, UpOutcome,
};
use crate::error::CboxError;

// Re-export crossterm's KeyEvent under the tui feature so message.rs stays
// importable in tests even without the feature (we wrap it in an Option-like
// enum below in that case). Since the reducer is always compiled, we use a
// thin wrapper.

/// Internal key representation — decouples the reducer from crossterm's type
/// (which is only available under the `tui` feature).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    CtrlC,
    CtrlD,
    Other,
}

/// Internal mouse representation — scroll-only, decoupled from crossterm.
/// No click, no hit-testing, no layout Rects (spec constraint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mouse {
    ScrollUp,
    ScrollDown,
    /// Any other mouse event — ignored by the reducer.
    Other,
}

/// Outcomes from create / rm that don't have a richer struct in core yet.
#[derive(Debug, Clone)]
pub struct CreateOutcome {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RmOutcome {
    pub removed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct StopOutcome {
    pub stopped: Vec<String>,
}

/// Every input the reducer understands.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Message {
    // ── input (normalized from crossterm) ──
    Key(Key),
    Tick,
    Resize(u16, u16),
    /// Bundle 3: mouse scroll event (scroll-only, no click).
    Mouse(Mouse),

    // ── effect completions (from the worker thread) ──
    ListLoaded(Result<Vec<BoxRow>, CboxError>),
    DetailLoaded(Result<InspectResult, CboxError>),
    CreateDone(Result<CreateOutcome, CboxError>),
    RmDone(Result<RmOutcome, CboxError>),
    StopDone(Result<StopOutcome, CboxError>),
    ApplyDone(Result<ApplyOutcome, CboxError>),
    UpDone(Result<UpOutcome, CboxError>),
    DoctorDone(Result<DoctorResult, CboxError>),

    // ── terminal-handoff completions ──
    EnterReturned(Result<i32, CboxError>),
    EditReturned(Result<(), CboxError>),

    // ── Bundle 2: silent poll completions ──
    /// Silent list refresh complete — does NOT set busy/status/toast on Ok.
    SilentListLoaded(Result<Vec<BoxRow>, CboxError>),
    /// Stats poll complete — feeds history buffer on Ok, swallowed silently on Err.
    StatsLoaded(Result<StatsSample, CboxError>),

    // ── Bundle 3: streaming log completions (from dedicated stream thread) ──
    /// A coalesced batch of log lines from the stream thread.
    LogChunk(Vec<String>),
    /// The log stream has ended (container stopped or cancelled). Contains the
    /// exit code of the `<backend> logs -f` process, if available.
    LogStreamEnded(Option<i32>),
}
