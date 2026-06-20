//! Bulk operation helpers — pure predicates over `&[BoxRow]`.
//!
//! Always compiled (no `tui` gate); lean-clean (no ratatui, no Effect, no nucleo).
//! Unit-testable without a terminal.
//!
//! GAP-2: bulk ops are FILTERS over `model.boxes`, not a Boxfile reconcile.
//! The fan-out reuses the existing multi-name `Effect::Rm`/`Effect::Stop`.
#![allow(dead_code)]

use crate::core::spec::BoxRow;

// ─── BulkOp ──────────────────────────────────────────────────────────────────

/// The four bulk operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkOp {
    /// Remove all stopped (non-running) boxes.
    PruneStopped,
    /// Stop all running boxes.
    StopRunning,
    /// Destroy all boxes that cbox created (`cbox_managed == true`).
    DestroyManaged,
    /// Destroy all boxes that cbox did NOT create (`cbox_managed == false`).
    /// DANGEROUS — requires a typed-phrase confirmation.
    DestroyUnmanaged,
}

// ─── is_running ──────────────────────────────────────────────────────────────

/// Shared "is this box running?" predicate — mirrors the reducer's inline check
/// at `update.rs:265-266` so List nav, Detail, and bulk all agree.
pub fn is_running(status: &str) -> bool {
    let s = status.to_lowercase();
    s.contains("running") || s.contains("up")
}

// ─── bulk_targets ────────────────────────────────────────────────────────────

/// Return the indices (into `boxes`) of the boxes targeted by `op`.
///
/// Pure function over `&[BoxRow]` — no I/O, no ratatui, testable in isolation.
///
/// AC-BULK-FILTER-1: given a known set, each op selects the expected subset.
pub fn bulk_targets(op: BulkOp, boxes: &[BoxRow]) -> Vec<usize> {
    boxes
        .iter()
        .enumerate()
        .filter(|(_, b)| match op {
            BulkOp::PruneStopped => !is_running(&b.status),
            BulkOp::StopRunning => is_running(&b.status),
            BulkOp::DestroyManaged => b.cbox_managed,
            BulkOp::DestroyUnmanaged => !b.cbox_managed,
        })
        .map(|(i, _)| i)
        .collect()
}
