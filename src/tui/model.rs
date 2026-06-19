//! Pure data model for the TUI (no I/O, no ratatui types except ListState).
//!
//! Available regardless of the `tui` feature so tests can import it.

use crate::core::spec::{BoxRow, DoctorResult, EnterSpec, InspectResult, ProvisionStepResult};
use crate::dbox::backend::Backend;
use crate::tui::theme::ColorMode;

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

/// Cozy status bar state — a typed enum so the view can color it and tests can assert
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

    pub fn move_up(&mut self) {
        if self.boxes.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            None => 0,
            Some(0) => 0,
            Some(i) => i - 1,
        });
    }

    pub fn move_down(&mut self) {
        if self.boxes.is_empty() {
            return;
        }
        let max = self.boxes.len() - 1;
        self.selected = Some(match self.selected {
            None => 0,
            Some(i) if i >= max => max,
            Some(i) => i + 1,
        });
    }
}
