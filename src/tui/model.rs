//! Pure data model for the TUI (no I/O, no ratatui types except ListState).
//!
//! Available regardless of the `tui` feature so tests can import it.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::core::spec::{
    BoxRow, DoctorResult, EnterSpec, InspectResult, ProvisionStepResult, StatsSample,
};
use crate::dbox::backend::Backend;
use crate::tui::bulk::BulkOp;
use crate::tui::cmdlog::CmdLog;
use crate::tui::theme::{ColorMode, Skin};

// ─── Screen ──────────────────────────────────────────────────────────────────

/// Which screen / panel is currently active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    List,
    Detail,
    Wizard,
    ConfirmDestroy,
    Progress,
    DoctorPanel,
}

// ─── StatusLine ──────────────────────────────────────────────────────────────

/// Status bar state — a typed enum so the view can color it and tests can assert
/// the variant without string-matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusLine {
    Idle,
    Busy(String),
    Ok(String),
    Error(String),
}

impl StatusLine {
    #[allow(dead_code)]
    pub fn is_error(&self) -> bool {
        matches!(self, StatusLine::Error(_))
    }
}

// ─── WizardState ─────────────────────────────────────────────────────────────

/// Which step the create wizard is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardStep {
    Name,
    Image,
    Packages,
    DockerMode,
    Confirm,
}

/// State for the create wizard.
#[derive(Debug, Clone)]
pub struct WizardState {
    pub step: WizardStep,
    pub name: String,
    pub image: String,
    pub packages_raw: String,
    pub docker_mode_idx: usize, // 0=none, 1=host, 2=nested
    pub dirty: bool,
}

impl WizardState {
    pub fn new() -> Self {
        WizardState {
            step: WizardStep::Name,
            name: String::new(),
            image: "registry.fedoraproject.org/fedora-toolbox:latest".to_string(),
            packages_raw: String::new(),
            docker_mode_idx: 0,
            dirty: false,
        }
    }

    pub fn docker_mode_str(&self) -> &'static str {
        match self.docker_mode_idx {
            1 => "host",
            2 => "nested",
            _ => "none",
        }
    }
}

impl Default for WizardState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── ConfirmState ────────────────────────────────────────────────────────────

/// State for the confirm-destroy modal.
#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub name: String,
    pub rm_home: bool,
    /// Engine the box lives on, so `rm` targets the right backend.
    pub backend: Backend,
}

// ─── ProgressState ───────────────────────────────────────────────────────────

/// State for the apply/up progress screen.
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub title: String,
    pub steps: Vec<ProvisionStepResult>,
    pub spinner_tick: usize,
    /// Set when recreate is needed but not yet confirmed.
    pub recreate_needed: bool,
    /// The recreate-class message from the error.
    pub recreate_msg: Option<String>,
    /// Whether we're showing the recreate confirm modal.
    pub recreate_confirm: bool,
    /// The ApplySpec being worked with (for re-issuing with recreate:true).
    pub pending_spec: Option<crate::core::spec::ApplySpec>,
}

// ─── Minimal list cursor (not ratatui ListState) ──────────────────────────────

/// Lightweight list cursor so the model stays pure (no ratatui dep).
#[derive(Debug, Clone, Default)]
pub struct ListCursor {
    #[allow(dead_code)]
    pub offset: usize,
}

// ─── FilterState (T3 – fuzzy filter) ─────────────────────────────────────────

/// State for the active fuzzy filter overlay.
/// `None` on Model means the filter is closed.
#[derive(Debug, Clone)]
pub struct FilterState {
    /// Raw user input (what they typed).
    pub query: String,
    /// Ordered indices into `model.boxes`, best-rank first.
    /// Empty query → all indices in original order.
    pub matches: Vec<usize>,
    /// Selection index within `matches` (0-based).
    pub cursor: usize,
}

impl FilterState {
    pub fn new() -> Self {
        FilterState {
            query: String::new(),
            matches: Vec::new(),
            cursor: 0,
        }
    }
}

impl Default for FilterState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Overlay (T2 – cheatsheet / command-log overlays) ────────────────────────

/// Which (if any) non-filter overlay is currently active.
/// Cheatsheet, CommandLog, and Palette are mutually exclusive — the enum enforces that.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    Cheatsheet,
    CommandLog {
        scroll: usize,
    },
    /// Command palette — fuzzy-searchable overlay mapping labels to actions.
    Palette {
        /// Raw user input (what they typed).
        query: String,
        /// Indices into the palette's action source, best-rank first.
        matches: Vec<usize>,
        /// Selection within `matches` (0-based).
        cursor: usize,
        /// When `true`, the source is restricted to the four bulk actions (opened via `b`).
        bulk_only: bool,
    },
}

// ─── BulkConfirmState ────────────────────────────────────────────────────────

/// State for the bulk-confirm modal.
#[derive(Debug, Clone)]
pub struct BulkConfirmState {
    pub op: BulkOp,
    /// Box NAMES selected by the predicate (for display and fan-out).
    pub targets: Vec<String>,
    /// Backends parallel to `targets` (same index — for grouped fan-out).
    pub target_backends: Vec<String>,
    /// Typed-phrase buffer (only meaningful for `DestroyUnmanaged`).
    pub typed_confirm: String,
}

// ─── StatsHistory ────────────────────────────────────────────────────────────

/// Maximum number of stats samples kept in the bounded ring buffers (~2 min of history).
pub const STATS_HISTORY_CAP: usize = 60;

/// Bounded stats history for the Detail screen sparklines.
#[derive(Debug, Clone)]
pub struct StatsHistory {
    /// Which box these samples belong to.
    pub box_id: String,
    /// CPU% × 100, rounded to u64 (Sparkline data).
    pub cpu: VecDeque<u64>,
    /// Memory used in bytes (Sparkline data).
    pub mem_used: VecDeque<u64>,
    /// Latest memory limit in bytes (for the scale label).
    pub mem_limit: u64,
}

impl StatsHistory {
    pub fn new(box_id: impl Into<String>) -> Self {
        Self {
            box_id: box_id.into(),
            cpu: VecDeque::new(),
            mem_used: VecDeque::new(),
            mem_limit: 0,
        }
    }

    /// Push a new sample into the bounded ring buffers.
    /// Drops the oldest sample when either buffer exceeds `STATS_HISTORY_CAP`.
    pub fn push_sample(&mut self, sample: &StatsSample) {
        // CPU stored as integer percentage × 100 (i.e. 12.5% → 1250).
        let cpu_val = (sample.cpu_pct * 100.0).round() as u64;
        self.cpu.push_back(cpu_val);
        if self.cpu.len() > STATS_HISTORY_CAP {
            self.cpu.pop_front();
        }

        self.mem_used.push_back(sample.mem_used);
        if self.mem_used.len() > STATS_HISTORY_CAP {
            self.mem_used.pop_front();
        }

        self.mem_limit = sample.mem_limit;
    }
}

// ─── Toast / ToastKind (T5 – transient notifications) ─────────────────────────

/// The semantic kind of a transient toast notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Info,
    Error,
}

/// A transient notification displayed for TTL ticks and then expired.
#[derive(Debug, Clone)]
pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    /// Value of `model.spinner_tick` when this toast was created.
    pub born_tick: usize,
    /// Expire when `(now_tick - born_tick) >= ttl_ticks`.
    /// Note: spinner_tick wraps (wrapping_add) but at TTL scale the wrap is benign.
    pub ttl_ticks: usize,
}

/// Default TTL in ticks for each kind (~50ms/tick when idle).
pub const TOAST_TTL_SUCCESS: usize = 60; // ~3 s
pub const TOAST_TTL_INFO: usize = 60; // ~3 s
pub const TOAST_TTL_ERROR: usize = 120; // ~6 s

/// Maximum number of simultaneously visible toasts (oldest dropped past cap).
pub const TOAST_MAX: usize = 3;

// ─── Model ───────────────────────────────────────────────────────────────────

/// The single source of truth for the TUI.
pub struct Model {
    pub screen: Screen,
    pub boxes: Vec<BoxRow>,
    pub selected: Option<usize>,
    #[allow(dead_code)]
    pub cursor: ListCursor,
    pub detail: Option<InspectResult>,
    pub wizard: Option<WizardState>,
    pub confirm: Option<ConfirmState>,
    pub progress: Option<ProgressState>,
    pub status: StatusLine,
    /// Default backend for *creating* new boxes (the preferred usable engine).
    pub backend: Backend,
    /// Every usable backend — listing merges boxes across all of these.
    pub backends: Vec<Backend>,
    pub doctor: Option<DoctorResult>,
    pub should_quit: bool,
    pub busy: bool,
    pub pending_enter: Option<EnterSpec>,
    pub pending_edit: Option<String>,
    pub spinner_tick: usize,
    /// Color capability detected at launch; used by `view` to build the `Theme`.
    /// Defaults to `TrueColor` inside `Model::new` (test-friendly); overridden
    /// by `app::run` after the real detection runs.
    pub color_mode: ColorMode,

    // ── Bundle 1 fields ──────────────────────────────────────────────────────
    /// Active fuzzy filter (`None` = filter closed).
    pub filter: Option<FilterState>,
    /// Which (if any) overlay is displayed on top of the current screen.
    pub overlay: Overlay,
    /// Current skin for theming. Defaults to `Skin::Kraft` (the shipped retro look).
    pub skin: Skin,
    /// Transient toast queue — additive over `status`. Bounded to `TOAST_MAX` entries.
    pub toasts: Vec<Toast>,
    /// Shared command-log ring buffer. Created in `app::run`; shared with `LoggingRunner`.
    pub cmdlog: Arc<Mutex<CmdLog>>,

    // ── Bundle 2 fields ──────────────────────────────────────────────────────
    /// Value of `spinner_tick` when the last silent poll was dispatched; init 0.
    pub last_poll_tick: usize,
    /// A silent effect is dispatched but not yet completed; init false.
    /// NEVER sets `busy` — that would block all keys (GAP-1).
    pub poll_in_flight: bool,
    /// Bulk-confirm modal state (`None` = modal closed).
    pub bulk_confirm: Option<BulkConfirmState>,
    /// Bounded stats history for the Detail screen sparklines.
    /// `None` until the first sample arrives / when not on Detail.
    pub stats_history: Option<StatsHistory>,
}

impl Model {
    pub fn new(backend: Backend) -> Self {
        Model {
            screen: Screen::List,
            boxes: Vec::new(),
            selected: None,
            cursor: ListCursor::default(),
            detail: None,
            wizard: None,
            confirm: None,
            progress: None,
            status: StatusLine::Idle,
            backends: vec![backend.clone()],
            backend,
            doctor: None,
            should_quit: false,
            busy: false,
            pending_enter: None,
            pending_edit: None,
            spinner_tick: 0,
            // Default to TrueColor so tests compile unchanged (Model::new(Backend::Podman)).
            // app::run overrides this with the result of theme::detect() at launch.
            color_mode: ColorMode::TrueColor,
            // Bundle 1 defaults.
            filter: None,
            overlay: Overlay::None,
            skin: Skin::Kraft,
            toasts: Vec::new(),
            cmdlog: Arc::new(Mutex::new(CmdLog::new(200))),
            // Bundle 2 defaults.
            last_poll_tick: 0,
            poll_in_flight: false,
            bulk_confirm: None,
            stats_history: None,
        }
    }

    /// Whether the box list is empty (after a successful load).
    /// Used in AC-EMPTY-1 test assertion.
    #[allow(dead_code)]
    pub fn is_empty_list(&self) -> bool {
        self.boxes.is_empty()
    }

    /// Currently selected box (if any).
    pub fn selected_box(&self) -> Option<&BoxRow> {
        self.selected.and_then(|i| self.boxes.get(i))
    }

    /// Returns the effective list of box indices for display/navigation.
    /// When a filter is active, returns `filter.matches`; otherwise all indices.
    pub fn filtered_indices(&self) -> Vec<usize> {
        match &self.filter {
            Some(f) => f.matches.clone(),
            None => (0..self.boxes.len()).collect(),
        }
    }

    pub fn move_up(&mut self) {
        if self.boxes.is_empty() {
            return;
        }
        match &mut self.filter {
            Some(f) => {
                if f.matches.is_empty() {
                    return;
                }
                if f.cursor > 0 {
                    f.cursor -= 1;
                }
                let idx = f.matches[f.cursor];
                self.selected = Some(idx);
            }
            None => {
                self.selected = Some(match self.selected {
                    None => 0,
                    Some(0) => 0,
                    Some(i) => i - 1,
                });
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.boxes.is_empty() {
            return;
        }
        match &mut self.filter {
            Some(f) => {
                if f.matches.is_empty() {
                    return;
                }
                let max_cursor = f.matches.len() - 1;
                if f.cursor < max_cursor {
                    f.cursor += 1;
                }
                let idx = f.matches[f.cursor];
                self.selected = Some(idx);
            }
            None => {
                let max = self.boxes.len() - 1;
                self.selected = Some(match self.selected {
                    None => 0,
                    Some(i) if i >= max => max,
                    Some(i) => i + 1,
                });
            }
        }
    }

    // ── Toast helpers (T5) ────────────────────────────────────────────────────

    /// Push a toast notification onto the queue. Bounded to TOAST_MAX — the oldest
    /// entry is dropped if the cap is exceeded.
    pub fn push_toast(&mut self, kind: ToastKind, text: String) {
        let ttl = match kind {
            ToastKind::Success => TOAST_TTL_SUCCESS,
            ToastKind::Info => TOAST_TTL_INFO,
            ToastKind::Error => TOAST_TTL_ERROR,
        };
        let toast = Toast {
            kind,
            text,
            born_tick: self.spinner_tick,
            ttl_ticks: ttl,
        };
        self.toasts.push(toast);
        // Drop oldest beyond cap.
        while self.toasts.len() > TOAST_MAX {
            self.toasts.remove(0);
        }
    }

    /// Set `model.status` to `Ok` AND push a matching success toast.
    pub fn set_status_ok(&mut self, msg: String) {
        self.status = StatusLine::Ok(msg.clone());
        self.push_toast(ToastKind::Success, msg);
    }

    /// Set `model.status` to `Error` AND push a matching error toast.
    pub fn set_status_error(&mut self, msg: String) {
        self.status = StatusLine::Error(msg.clone());
        self.push_toast(ToastKind::Error, msg);
    }
}
