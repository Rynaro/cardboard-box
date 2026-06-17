use super::runner::{CmdOutput, DistroboxRunner, Invocation, RunMode, RunnerError};
use std::process::{Command, Stdio};

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
