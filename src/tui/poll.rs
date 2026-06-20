//! Silent-poll gating — pure helper that decides when to emit a background poll.
//!
//! Always compiled (no `tui` gate); lean-clean (no ratatui, no Effect, no nucleo).
//! Returns a `PollKind` rather than an `Effect` to avoid dragging `Effect`/spec
//! types into the lean module. The reducer maps `PollKind` → `Effect`.
//!
//! GAP-1 (poll side): the poll MUST NOT set `model.busy`; it uses `poll_in_flight`
//! as the in-flight guard. A single outstanding silent effect is allowed at a time.
#![allow(dead_code)]

// ─── Constants ────────────────────────────────────────────────────────────────

/// Number of Ticks between silent polls (~2 s at POLL_MS=50 per `app.rs:41`).
pub const POLL_INTERVAL_TICKS: usize = 40;

/// Wall-clock timeout for the stats engine call (tighter — engine sockets are
/// the most likely to wedge).
pub const STATS_TIMEOUT_SECS: u64 = 3;

/// Wall-clock timeout for the silent list refresh call.
pub const LIST_TIMEOUT_SECS: u64 = 5;

// ─── PollKind ────────────────────────────────────────────────────────────────

/// What the next background poll should fetch.
///
/// The reducer maps this to the concrete `Effect` variant so `poll.rs` stays
/// free of the `Effect` type and lean-clean (R-7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollKind {
    /// Refresh the full box list silently.
    List,
    /// Fetch per-box CPU/mem stats for the box currently in Detail.
    Stats {
        /// Container ID (BoxRow.id).
        id: String,
        /// Backend name (BoxRow.backend).
        backend: String,
    },
}

// ─── Model view ──────────────────────────────────────────────────────────────

/// Minimal view of the Model fields that `should_poll` needs, so `poll.rs` can
/// stay free of the `Model` type in lean builds.
///
/// The reducer constructs this from `&Model` inline when calling `should_poll`.
pub struct PollGate {
    pub spinner_tick: usize,
    pub last_poll_tick: usize,
    pub busy: bool,
    pub poll_in_flight: bool,
    /// `Some((id, backend))` when the user is on the Detail screen viewing a
    /// RUNNING box; `None` otherwise (stops box or list/other screen).
    pub detail_running: Option<(String, String)>,
}

// ─── should_poll ─────────────────────────────────────────────────────────────

/// Pure gating function: returns `Some(PollKind)` when a background poll should
/// fire, `None` when it should be skipped.
///
/// Fires IFF ALL of:
/// 1. `spinner_tick - last_poll_tick >= POLL_INTERVAL_TICKS`
/// 2. `!busy`  (a user op is running — defer to avoid racing a manual LoadList)
/// 3. `!poll_in_flight`  (a previous silent effect hasn't completed yet — coalesce)
///
/// When on Detail with a running box → prefers `Stats`; otherwise → `List`.
///
/// AC-POLL-1/2/3.
pub fn should_poll(gate: &PollGate) -> Option<PollKind> {
    let elapsed = gate.spinner_tick.wrapping_sub(gate.last_poll_tick);
    if elapsed < POLL_INTERVAL_TICKS {
        return None;
    }
    if gate.busy {
        return None;
    }
    if gate.poll_in_flight {
        return None;
    }

    // Prefer Stats when on Detail with a running box.
    if let Some((id, backend)) = &gate.detail_running {
        return Some(PollKind::Stats {
            id: id.clone(),
            backend: backend.clone(),
        });
    }

    Some(PollKind::List)
}
