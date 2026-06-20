//! Cross-session action history: `HistoryStore` + `HistoryEntry` + `redact_argv`.
//!
//! LEAN module — no ratatui dep, no tui feature gate, so it compiles under
//! `--no-default-features` (G-BUILD-LEAN).
//!
//! Persistence: `$XDG_STATE_HOME/cbox/history.jsonl` (default `~/.local/state`),
//! append-only writes, cap `HISTORY_CAP`, serde_json. Redaction runs before
//! every write so secrets never reach disk (G-REDACT).
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use std::path::PathBuf;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of history entries retained on disk.
pub const HISTORY_CAP: usize = 1000;

/// The JSONL file name inside the cbox state directory.
const HISTORY_FILE: &str = "history.jsonl";

// ─── HistoryEntry ─────────────────────────────────────────────────────────────

/// A single persisted history entry (already redacted before write).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Space-joined argv, redacted (secrets replaced with `<redacted>`).
    pub argv: String,
    /// Exit code; `None` for interactive spawns.
    pub status: Option<i32>,
    /// Unix timestamp (seconds since epoch) at the time of the spawn.
    pub ts: i64,
}

// ─── HistoryStore ─────────────────────────────────────────────────────────────

/// Host-side persistent history store.
///
/// Located at `$XDG_STATE_HOME/cbox/history.jsonl` (default `~/.local/state`).
/// Append-only writes; compaction to `HISTORY_CAP` when the file grows large.
/// Corrupt/missing file → empty history, NEVER panic (G-HISTSAFE).
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    /// Resolve the history file path from the environment.
    ///
    /// Mirrors the XDG_CONFIG_HOME fallback pattern in `update.rs:1299-1303`
    /// but for `XDG_STATE_HOME` → `~/.local/state`.
    pub fn resolve_path() -> PathBuf {
        let state_home = std::env::var("XDG_STATE_HOME").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            format!("{home}/.local/state")
        });
        PathBuf::from(state_home).join("cbox").join(HISTORY_FILE)
    }

    /// Create a store pointing at the default XDG path.
    pub fn new() -> Self {
        HistoryStore {
            path: Self::resolve_path(),
        }
    }

    /// Create a store pointing at a custom path (for tests).
    pub fn with_path(path: PathBuf) -> Self {
        HistoryStore { path }
    }

    /// Append one redacted entry to disk (best-effort; silently drops on error).
    ///
    /// Creates the parent directory if absent. The entry is NOT redacted here —
    /// the caller must call `redact_argv` first (the LoggingRunner hook does this).
    pub fn append(&self, argv: String, status: Option<i32>) {
        let entry = HistoryEntry {
            argv,
            status,
            ts: unix_now(),
        };

        // Ensure parent dir exists (best-effort).
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Amortized compaction: if file line-count exceeds 2×cap, rewrite.
        // We do this lazily before appending to keep append O(1) normally.
        self.maybe_compact();

        // Append one JSON line.
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            if let Ok(line) = serde_json::to_string(&entry) {
                let _ = writeln!(file, "{line}");
            }
        }
    }

    /// Load the most recent `HISTORY_CAP` valid entries from disk.
    ///
    /// - Missing file → `Ok(vec![])`, no panic.
    /// - Corrupt/partial lines → skipped, valid entries returned, no panic.
    /// - Bounded: reads the whole file but parses only into memory up to cap.
    pub fn load(&self) -> Vec<HistoryEntry> {
        load_from_path(&self.path, HISTORY_CAP)
    }

    /// Compact the file to `HISTORY_CAP` entries when it exceeds `2 * HISTORY_CAP`.
    fn maybe_compact(&self) {
        // Fast-path: stat the file; if it is small, skip the compact.
        let line_estimate = self
            .path
            .metadata()
            .ok()
            .map(|m| m.len() / 60) // rough 60-byte-per-line estimate
            .unwrap_or(0);

        if line_estimate < (2 * HISTORY_CAP) as u64 {
            return;
        }

        let entries = load_from_path(&self.path, HISTORY_CAP);
        if entries.is_empty() {
            return;
        }

        // Rewrite atomically: write to a temp file, rename.
        let tmp = self.path.with_extension("jsonl.tmp");
        if let Ok(mut file) = std::fs::File::create(&tmp) {
            for e in &entries {
                if let Ok(line) = serde_json::to_string(e) {
                    let _ = writeln!(file, "{line}");
                }
            }
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Load at most `cap` valid entries from the JSONL file at `path`.
fn load_from_path(path: &PathBuf, cap: usize) -> Vec<HistoryEntry> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(), // missing → empty, no panic
    };

    let reader = std::io::BufReader::new(file);
    let mut entries: Vec<HistoryEntry> = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue, // corrupt read → skip
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<HistoryEntry>(trimmed) {
            Ok(e) => entries.push(e),
            Err(_) => continue, // unparseable line → skip, no panic
        }
    }

    // Return only the most recent `cap` entries (oldest dropped).
    if entries.len() > cap {
        entries.drain(..entries.len() - cap);
    }
    entries
}

// ─── redact_argv ─────────────────────────────────────────────────────────────

/// Scrub secret-bearing values from a space-joined argv string.
///
/// Rules (spec §6.2):
/// 1. `KEY=VALUE` where KEY (case-insensitive) contains any of the secret-name
///    patterns → `KEY=<redacted>`.
/// 2. `--env KEY=VALUE` (plaintext env inline, argv.rs:98-102) → `--env KEY=<redacted>`
///    unconditionally (env values may be sensitive).
/// 3. Everything else passes through verbatim.
///
/// The output is what gets written to disk AND shown in the History overlay.
pub fn redact_argv(argv: &str) -> String {
    let tokens: Vec<&str> = argv.split(' ').collect();
    let mut result = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        let tok = tokens[i];

        // Rule 2: `--env KEY=VALUE` triple (--env is a standalone flag followed by the kv).
        // The form argv.rs:98-102 is `--additional-flags --env KEY=VALUE` but it can also
        // appear as `--env KEY=VALUE` directly in the joined string.
        if tok == "--env" {
            if let Some(next) = tokens.get(i + 1) {
                if next.contains('=') {
                    // `--env KEY=VALUE` — redact the value half.
                    let redacted = redact_kv(next);
                    result.push("--env".to_string());
                    result.push(redacted);
                    i += 2;
                    continue;
                }
                // `--env KEY` (name-only, persist=true form) — pass through.
                result.push(tok.to_string());
                result.push(next.to_string());
                i += 2;
                continue;
            }
            // Lone `--env` with nothing after — pass through.
            result.push(tok.to_string());
            i += 1;
            continue;
        }

        // Rule 1 & inline `--env KEY=VALUE` embedded as a single token:
        // e.g. `--env KEY=VALUE` could be a single token from argv.rs:98-102 building
        // `"--env {key}={value}"` as one string passed as `--additional-flags`.
        // Handle any token that starts with `--env ` or is a bare `KEY=VALUE`.
        if let Some(rest) = tok.strip_prefix("--env ") {
            // Single-token form `--env KEY=VALUE` (embedded as one token).
            if rest.contains('=') {
                result.push(format!("--env {}", redact_kv(rest)));
            } else {
                result.push(tok.to_string());
            }
            i += 1;
            continue;
        }

        // Rule 1: standalone `KEY=VALUE` token where KEY is secret-shaped.
        if tok.contains('=') {
            result.push(maybe_redact_kv(tok));
            i += 1;
            continue;
        }

        result.push(tok.to_string());
        i += 1;
    }

    result.join(" ")
}

/// Secret-name deny patterns (case-insensitive substrings of the KEY part).
const SECRET_PATTERNS: &[&str] = &[
    "token", "secret", "password", "passwd", "apikey", "api_key", "api-key", "pass",
];

/// Redact `KEY=VALUE` unconditionally (used for `--env` values).
fn redact_kv(kv: &str) -> String {
    match kv.find('=') {
        Some(pos) => format!("{}=<redacted>", &kv[..pos]),
        None => kv.to_string(),
    }
}

/// Redact `KEY=VALUE` only if KEY matches the deny-pattern list.
fn maybe_redact_kv(kv: &str) -> String {
    match kv.find('=') {
        Some(pos) => {
            let key_lower = kv[..pos].to_lowercase();
            if SECRET_PATTERNS.iter().any(|p| key_lower.contains(p)) {
                format!("{}=<redacted>", &kv[..pos])
            } else {
                kv.to_string()
            }
        }
        None => kv.to_string(),
    }
}

// ─── unix_now ─────────────────────────────────────────────────────────────────

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
