use super::runner::{CmdOutput, DistroboxRunner, Invocation, RunMode, RunnerError};
use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Poll grain for the `try_wait` watchdog loop (~25ms → kill latency ≤ one grain past deadline).
#[allow(dead_code)]
const POLL_GRAIN_MS: u64 = 25;

/// Real runner — uses std::process::Command.
pub struct RealRunner;

impl DistroboxRunner for RealRunner {
    fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError> {
        let argv = inv.argv();

        if inv.mode == RunMode::DryRun {
            return Ok(CmdOutput {
                status: 0,
                stdout: argv.join(" "),
                stderr: String::new(),
                argv,
            });
        }

        let mut cmd = Command::new(&inv.program);
        cmd.args(&inv.args);
        for (k, v) in &inv.env {
            cmd.env(k, v);
        }

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RunnerError::BinaryNotFound {
                        program: inv.program.clone(),
                    }
                } else {
                    RunnerError::Io {
                        program: inv.program.clone(),
                        source: e,
                    }
                }
            })?;

        let status = output.status.code().unwrap_or(-1);
        Ok(CmdOutput {
            status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            argv,
        })
    }

    fn run_with_timeout(
        &self,
        inv: Invocation,
        timeout: Duration,
    ) -> Result<CmdOutput, RunnerError> {
        // DryRun short-circuits exactly as run() does (no spawn needed).
        if inv.mode == RunMode::DryRun {
            return self.run(inv);
        }

        let argv = inv.argv();

        let mut child = Command::new(&inv.program)
            .args(&inv.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RunnerError::BinaryNotFound {
                        program: inv.program.clone(),
                    }
                } else {
                    RunnerError::Io {
                        program: inv.program.clone(),
                        source: e,
                    }
                }
            })?;

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => break, // finished in time → collect output below
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait(); // reap to avoid zombie
                        return Err(RunnerError::Timeout {
                            program: inv.program.clone(),
                            seconds: timeout.as_secs(),
                        });
                    }
                    std::thread::sleep(Duration::from_millis(POLL_GRAIN_MS));
                }
                Err(e) => {
                    return Err(RunnerError::Io {
                        program: inv.program.clone(),
                        source: e,
                    })
                }
            }
        }

        // Child finished within the deadline — collect output.
        let output = child.wait_with_output().map_err(|e| RunnerError::Io {
            program: inv.program.clone(),
            source: e,
        })?;

        let status = output.status.code().unwrap_or(-1);
        Ok(CmdOutput {
            status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            argv,
        })
    }

    fn run_stream(
        &self,
        inv: Invocation,
        on_line: &mut dyn FnMut(String),
        stop: &AtomicBool,
    ) -> Result<i32, RunnerError> {
        let mut child = Command::new(&inv.program)
            .args(&inv.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    RunnerError::BinaryNotFound {
                        program: inv.program.clone(),
                    }
                } else {
                    RunnerError::Io {
                        program: inv.program.clone(),
                        source: e,
                    }
                }
            })?;

        let stdout = child.stdout.take().expect("stdout piped");
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();

        loop {
            // Check cancel flag before each read.
            if stop.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait(); // reap to avoid zombie
                return Ok(-1);
            }

            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF — container stopped; reap and return exit code.
                    let status = child.wait().map_err(|e| RunnerError::Io {
                        program: inv.program.clone(),
                        source: e,
                    })?;
                    return Ok(status.code().unwrap_or(-1));
                }
                Ok(_) => {
                    // Trim trailing newline before delivering.
                    let trimmed = line.trim_end_matches(['\n', '\r']).to_string();
                    on_line(trimmed);
                }
                Err(e) => {
                    // IO error on read — reap and return error.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunnerError::Io {
                        program: inv.program.clone(),
                        source: e,
                    });
                }
            }
        }
    }

    fn run_interactive(&self, inv: Invocation) -> Result<i32, RunnerError> {
        let mut cmd = Command::new(&inv.program);
        cmd.args(&inv.args);
        for (k, v) in &inv.env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let status = cmd.status().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RunnerError::BinaryNotFound {
                    program: inv.program.clone(),
                }
            } else {
                RunnerError::InteractiveSpawnFailed {
                    program: inv.program.clone(),
                    source: e,
                }
            }
        })?;

        Ok(status.code().unwrap_or(-1))
    }
}
