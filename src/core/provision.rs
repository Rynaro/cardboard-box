//! Provision execution engine (§5).
//! Walks [[provision]] steps in Boxfile order: hash, skip-or-run, write state.

use crate::boxfile::model::{ProvisionStep, ProvisionType};
use crate::core::spec::ProvisionStepResult;
use crate::core::state_store::{epoch_secs, AppliedStep, ProvisionState, ProvisionStateStore};
use crate::dbox::{
    argv::{build_copy_argv, build_provision_shell_argv},
    backend::Backend,
    runner::{DistroboxRunner, Invocation, RunMode},
};
use crate::error::CboxError;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;

// ─── ProvisionPlan ────────────────────────────────────────────────────────────

/// Everything the engine needs to run a provision walk.
pub struct ProvisionPlan<'a> {
    pub name: &'a str,
    pub steps: &'a [ProvisionStep],
    pub boxfile_dir: &'a Path,
    pub backend: &'a Backend,
    pub force: bool,
    pub redo: &'a [usize],
    pub dry_run: bool,
}

// ─── Hashing (§5.2) ──────────────────────────────────────────────────────────

/// Compute the content hash for a provision step.
///
/// `shell`: sha256("shell\n" + normalize(run))
/// `copy`:  sha256("copy\n" + sha256(host_file_bytes(src)) + "\n" + dst)
///
/// Returns `Ok(hex_string)` or `Err(CboxError)` if a file can't be read.
pub fn hash_step(step: &ProvisionStep, boxfile_dir: &Path) -> Result<String, CboxError> {
    match step.step_type {
        ProvisionType::Shell => {
            let run = step.run.as_deref().unwrap_or("");
            let normalized = normalize_shell_run(run);
            let input = format!("shell\n{normalized}");
            Ok(hex_sha256(input.as_bytes()))
        }
        ProvisionType::Copy => {
            let src = step.src.as_deref().unwrap_or("");
            let dst = step.dst.as_deref().unwrap_or("");
            let src_path = resolve_src(src, boxfile_dir);
            let src_bytes = std::fs::read(&src_path).map_err(|e| {
                CboxError::ioerr(format!(
                    "Cannot read copy source \"{}\": {e}",
                    src_path.display()
                ))
            })?;
            let src_hash = hex_sha256(&src_bytes);
            let input = format!("copy\n{src_hash}\n{dst}");
            Ok(hex_sha256(input.as_bytes()))
        }
    }
}

/// Normalize a shell `run` string for hashing:
/// trim trailing whitespace per line + ensure single trailing newline.
/// Leading whitespace is preserved (semantically meaningful in heredocs).
pub fn normalize_shell_run(run: &str) -> String {
    // Trim trailing whitespace from each line
    let trimmed: Vec<String> = run.lines().map(|l| l.trim_end().to_string()).collect();
    // Rejoin with \n, strip trailing blank lines, ensure single trailing \n
    let rejoined = trimmed.join("\n");
    let stripped = rejoined.trim_end_matches('\n');
    format!("{stripped}\n")
}

fn hex_sha256(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let result = hasher.finalize();
    hex_encode(&result)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn resolve_src(src: &str, boxfile_dir: &Path) -> PathBuf {
    let p = Path::new(src);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        boxfile_dir.join(p)
    }
}

// ─── Engine ───────────────────────────────────────────────────────────────────

/// Walk the provision steps: hash, skip-or-run, write state after each success.
/// Returns a Vec<ProvisionStepResult> for all attempted steps.
/// Stops on first failure (§7.1).
pub fn provision(
    plan: &ProvisionPlan<'_>,
    store: &dyn ProvisionStateStore,
    runner: &dyn DistroboxRunner,
    state: &mut ProvisionState,
) -> Result<Vec<ProvisionStepResult>, CboxError> {
    let mut results: Vec<ProvisionStepResult> = Vec::new();

    for (idx, step) in plan.steps.iter().enumerate() {
        // Compute hash (copy steps need file access — pre-flight check here too)
        let hash = match step.step_type {
            ProvisionType::Copy => {
                // Pre-flight: check src exists on host
                let src = step.src.as_deref().unwrap_or("");
                let src_path = resolve_src(src, plan.boxfile_dir);
                if !src_path.exists() {
                    return Err(CboxError::dataerr(format!(
                        "Copy step [{idx}]: source \"{}\" not found on host",
                        src_path.display()
                    )));
                }
                hash_step(step, plan.boxfile_dir)?
            }
            ProvisionType::Shell => hash_step(step, plan.boxfile_dir)?,
        };

        // Decide: skip or run?
        let should_skip =
            !plan.force && !plan.redo.contains(&idx) && state.step_hash(idx) == Some(hash.as_str());

        if should_skip {
            results.push(ProvisionStepResult {
                idx,
                step_type: step_type_str(&step.step_type),
                status: "skipped".to_string(),
                hash,
                duration_ms: 0,
                exit_code: None,
                captured_stderr: String::new(),
                captured_stdout: String::new(),
                argv: Vec::new(),
            });
            continue;
        }

        // Run or dry-run
        let start = Instant::now();
        let step_result = run_step(plan, step, idx, &hash, runner)?;
        let duration_ms = start.elapsed().as_millis() as u64;

        if step_result.status == "failed" {
            let exit_code = step_result.exit_code.unwrap_or(1);
            let step_type = step_result.step_type.clone();
            // Choose the best output to surface: stderr preferred, stdout as fallback.
            let raw_output = if !step_result.captured_stderr.is_empty() {
                &step_result.captured_stderr
            } else {
                &step_result.captured_stdout
            };
            let surfaced = tail_lines(raw_output, 40);
            let step_argv = step_result.argv.clone();

            results.push(ProvisionStepResult {
                duration_ms,
                ..step_result
            });
            // Record failure in state so it re-runs next apply (hash stored with
            // result="failed"; step_hash() filters on result="ok", so it is not skipped).
            state.set_step(AppliedStep {
                idx,
                step_type: step_type_str(&step.step_type),
                hash: hash.clone(),
                applied_at: epoch_secs(),
                result: "failed".to_string(),
            });
            let _ = store.write(plan.name, state, runner); // best-effort

            // Build a rich error: headline + captured output + step argv.
            return Err(CboxError::provision_step_error(
                idx, &step_type, exit_code, &surfaced, &step_argv,
            ));
        }

        // Success — update state and write incrementally
        state.set_step(AppliedStep {
            idx,
            step_type: step_type_str(&step.step_type),
            hash: hash.clone(),
            applied_at: epoch_secs(),
            result: "ok".to_string(),
        });
        let _ = store.write(plan.name, state, runner); // best-effort; if write fails, step re-runs next apply

        results.push(ProvisionStepResult {
            duration_ms,
            ..step_result
        });
    }

    Ok(results)
}

fn run_step(
    plan: &ProvisionPlan<'_>,
    step: &ProvisionStep,
    idx: usize,
    hash: &str,
    runner: &dyn DistroboxRunner,
) -> Result<ProvisionStepResult, CboxError> {
    match step.step_type {
        ProvisionType::Shell => {
            let run = step.run.as_deref().unwrap_or("");
            let args = build_provision_shell_argv(plan.name, run);
            let mode = if plan.dry_run {
                RunMode::DryRun
            } else {
                RunMode::Capture
            };
            let inv = Invocation::new("distrobox", args, mode);
            let out = runner.run(inv)?;

            if out.status != 0 && !plan.dry_run {
                return Ok(ProvisionStepResult {
                    idx,
                    step_type: "shell".to_string(),
                    status: "failed".to_string(),
                    hash: hash.to_string(),
                    duration_ms: 0,
                    exit_code: Some(out.status),
                    captured_stderr: out.stderr.clone(),
                    captured_stdout: out.stdout.clone(),
                    argv: out.argv.clone(),
                });
            }

            Ok(ProvisionStepResult {
                idx,
                step_type: "shell".to_string(),
                status: "ran".to_string(),
                hash: hash.to_string(),
                duration_ms: 0,
                exit_code: Some(out.status),
                captured_stderr: String::new(),
                captured_stdout: String::new(),
                argv: out.argv.clone(),
            })
        }
        ProvisionType::Copy => {
            let src = step.src.as_deref().unwrap_or("");
            let dst = step.dst.as_deref().unwrap_or("");
            let src_path = resolve_src(src, plan.boxfile_dir);
            let src_str = src_path.to_string_lossy().to_string();

            let args = build_copy_argv(plan.name, &src_str, dst);
            let mode = if plan.dry_run {
                RunMode::DryRun
            } else {
                RunMode::Capture
            };
            let inv = Invocation::new(plan.backend.as_str(), args, mode);
            let out = runner.run(inv)?;

            if out.status != 0 && !plan.dry_run {
                return Ok(ProvisionStepResult {
                    idx,
                    step_type: "copy".to_string(),
                    status: "failed".to_string(),
                    hash: hash.to_string(),
                    duration_ms: 0,
                    exit_code: Some(out.status),
                    captured_stderr: out.stderr.clone(),
                    captured_stdout: out.stdout.clone(),
                    argv: out.argv.clone(),
                });
            }

            Ok(ProvisionStepResult {
                idx,
                step_type: "copy".to_string(),
                status: "copied".to_string(),
                hash: hash.to_string(),
                duration_ms: 0,
                exit_code: Some(out.status),
                captured_stderr: String::new(),
                captured_stdout: String::new(),
                argv: out.argv.clone(),
            })
        }
    }
}

fn step_type_str(t: &ProvisionType) -> String {
    match t {
        ProvisionType::Shell => "shell".to_string(),
        ProvisionType::Copy => "copy".to_string(),
    }
}

/// Return the last `n` lines of `text`.  If the text was truncated, prepends a
/// one-line note so the user knows they're seeing a tail, not the full output.
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= n {
        return text.to_string();
    }
    let tail = lines[lines.len() - n..].join("\n");
    format!(
        "[… {} line(s) truncated; showing last {n} …]\n{tail}",
        lines.len() - n
    )
}
