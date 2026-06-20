//! Pure data model for the TUI (no I/O, no ratatui types except ListState).
//!
//! Available regardless of the `tui` feature so tests can import it.

use std::sync::{Arc, Mutex};

use crate::core::spec::{BoxRow, DoctorResult, EnterSpec, InspectResult, ProvisionStepResult};
use crate::dbox::backend::Backend;
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
/// Cheatsheet and CommandLog are mutually exclusive — the enum enforces that.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    None,
    Cheatsheet,
    CommandLog {
        scroll: usize,
    },
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
