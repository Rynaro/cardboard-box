//! MockRunner — programmable double for testing. Available under `cfg(test)` or the
//! `testkit` feature so CI can drive all acceptance criteria without real distrobox.

#![allow(dead_code)]

use super::runner::{CmdOutput, DistroboxRunner, Invocation, RunnerError};
use std::sync::Mutex;

/// A recorded invocation call.
#[derive(Debug, Clone)]
pub struct RecordedCall {
    pub program: String,
    pub args: Vec<String>,
    pub interactive: bool,
}

/// A canned response for `run()`.
#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl MockResponse {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self {
            status: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    pub fn err(status: i32, stderr: impl Into<String>) -> Self {
        Self {
            status,
            stdout: String::new(),
            stderr: stderr.into(),
        }
    }
}

/// A matcher: match on program + an args-subsequence.
pub struct MockMatcher {
    pub program: Option<String>,
    /// All these strings must appear in args (in any position).
    pub args_contain: Vec<String>,
    pub response: MockResponse,
    /// For run_interactive, return this exit code.
    pub interactive_exit: i32,
}

impl MockMatcher {
    pub fn new(response: MockResponse) -> Self {
        Self {
            program: None,
            args_contain: Vec::new(),
            response,
            interactive_exit: 0,
        }
    }

    pub fn with_program(mut self, program: impl Into<String>) -> Self {
        self.program = Some(program.into());
        self
    }

    pub fn with_args_contain(mut self, args: Vec<String>) -> Self {
        self.args_contain = args;
        self
    }

    pub fn with_interactive_exit(mut self, code: i32) -> Self {
        self.interactive_exit = code;
        self
    }

    fn matches(&self, inv: &Invocation) -> bool {
        if let Some(ref prog) = self.program {
            if inv.program != *prog {
                return false;
            }
        }
        for needle in &self.args_contain {
            if !inv.args.iter().any(|a| a == needle) {
                return false;
            }
        }
        true
    }
}

/// Programmable MockRunner. Queue matchers; first match wins.
pub struct MockRunner {
    matchers: Vec<MockMatcher>,
    calls: Mutex<Vec<RecordedCall>>,
    /// Default response if no matcher fires.
    default: MockResponse,
    /// Default interactive exit if no matcher fires.
    default_interactive_exit: i32,
}

impl MockRunner {
    pub fn new() -> Self {
        Self {
            matchers: Vec::new(),
            calls: Mutex::new(Vec::new()),
            default: MockResponse::ok(""),
            default_interactive_exit: 0,
        }
    }

    pub fn with_matcher(mut self, m: MockMatcher) -> Self {
        self.matchers.push(m);
        self
    }

    pub fn with_default(mut self, r: MockResponse) -> Self {
        self.default = r;
        self
    }

    pub fn with_default_interactive(mut self, code: i32) -> Self {
        self.default_interactive_exit = code;
        self
    }

    /// All recorded calls (both run and run_interactive).
    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Number of times the runner was invoked.
    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn record(&self, inv: &Invocation, interactive: bool) {
        self.calls.lock().unwrap().push(RecordedCall {
            program: inv.program.clone(),
            args: inv.args.clone(),
            interactive,
        });
    }

    fn find_matcher(&self, inv: &Invocation) -> Option<&MockMatcher> {
        self.matchers.iter().find(|m| m.matches(inv))
    }
}

impl Default for MockRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl DistroboxRunner for MockRunner {
    fn run(&self, inv: Invocation) -> Result<CmdOutput, RunnerError> {
        self.record(&inv, false);
        let argv = inv.argv();
        let resp = self
            .find_matcher(&inv)
            .map(|m| m.response.clone())
            .unwrap_or_else(|| self.default.clone());
        Ok(CmdOutput {
            status: resp.status,
            stdout: resp.stdout,
            stderr: resp.stderr,
            argv,
        })
    }

    fn run_interactive(&self, inv: Invocation) -> Result<i32, RunnerError> {
        self.record(&inv, true);
        let code = self
            .find_matcher(&inv)
            .map(|m| m.interactive_exit)
            .unwrap_or(self.default_interactive_exit);
        Ok(code)
    }
}
