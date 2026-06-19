//! Cozy chrome renderer + --json emitter + NO_COLOR / TTY detection.

use std::io::IsTerminal;

/// Rendering context that carries global flags.
pub struct OutputCtx {
    pub json: bool,
    pub quiet: bool,
    pub verbose: u8,
    pub no_color: bool,
}

impl OutputCtx {
    pub fn new(json: bool, quiet: bool, verbose: u8, no_color: bool) -> Self {
        let no_color =
            no_color || std::env::var("NO_COLOR").is_ok() || !std::io::stdout().is_terminal();
        Self {
            json,
            quiet,
            verbose,
            no_color,
        }
    }

    pub fn color(&self) -> bool {
        !self.no_color
    }

    /// Print to stdout (machine-clean; no chrome).
    #[allow(dead_code)]
    pub fn print_stdout(&self, s: &str) {
        println!("{s}");
    }

    /// Print to stderr (chrome / logs / errors).
    #[allow(dead_code)]
    pub fn print_stderr(&self, s: &str) {
        eprintln!("{s}");
    }

    /// Print a cozy success line with a check mark (or plain if quiet/no-color).
    pub fn success(&self, msg: &str) {
        if self.quiet {
            return;
        }
        if self.color() {
            println!("\x1b[32m\u{2713}\x1b[0m {msg}");
        } else {
            println!("ok: {msg}");
        }
    }

    /// Print an info hint (subdued).
    pub fn hint(&self, msg: &str) {
        if self.quiet {
            return;
        }
        if self.color() {
            println!("  \x1b[2m{msg}\x1b[0m");
        } else {
            println!("  {msg}");
        }
    }

    /// Print a warning to stderr.
    pub fn warn(&self, msg: &str) {
        if self.color() {
            eprintln!("\x1b[33mwarn:\x1b[0m {msg}");
        } else {
            eprintln!("warn: {msg}");
        }
    }

    /// Print a JSON value to stdout (the one valid stdout emission in --json mode).
    pub fn print_json<T: serde::Serialize>(&self, value: &T) {
        match serde_json::to_string_pretty(value) {
            Ok(s) => println!("{s}"),
            Err(e) => eprintln!("error: failed to serialize JSON: {e}"),
        }
    }

    /// Emit the exact argv when --verbose is set.
    #[allow(dead_code)]
    pub fn verbose_argv(&self, argv: &[String]) {
        if self.verbose >= 1 {
            eprintln!("+ {}", argv.join(" "));
        }
    }
}

/// Render a list table to stdout.
pub fn render_list_table(boxes: &[crate::core::spec::BoxRow], ctx: &OutputCtx) {
    if boxes.is_empty() {
        if !ctx.quiet {
            println!("No boxes found.");
        }
        return;
    }

    // Column widths
    let name_w = boxes.iter().map(|b| b.name.len()).max().unwrap_or(4).max(4);
    let status_w = boxes
        .iter()
        .map(|b| b.status.len())
        .max()
        .unwrap_or(6)
        .max(6);
    let image_w = boxes
        .iter()
        .map(|b| b.image.len())
        .max()
        .unwrap_or(5)
        .max(5);
    let backend_w = boxes
        .iter()
        .map(|b| b.backend.len())
        .max()
        .unwrap_or(7)
        .max(7);

    let header = format!(
        "{:<name_w$}  {:<backend_w$}  {:<status_w$}  {:<image_w$}  {:<8}  {}",
        "NAME", "BACKEND", "STATUS", "IMAGE", "DOCKER", "CBOX?",
    );

    if ctx.color() {
        println!("\x1b[1m{header}\x1b[0m");
    } else {
        println!("{header}");
    }
    println!("{}", "-".repeat(header.len()));

    for b in boxes {
        let cbox_mark = if b.cbox_managed { "yes" } else { "no" };
        println!(
            "{:<name_w$}  {:<backend_w$}  {:<status_w$}  {:<image_w$}  {:<8}  {}",
            b.name, b.backend, b.status, b.image, b.docker_mode, cbox_mark,
        );
    }
}

/// Render a cozy inspect panel.
pub fn render_inspect_panel(r: &crate::core::spec::InspectResult, ctx: &OutputCtx) {
    let sep = if ctx.color() {
        "\x1b[2m│\x1b[0m"
    } else {
        "|"
    };
    println!("{sep} Box:       {}", r.name);
    println!("{sep} Status:    {}", r.status);
    println!("{sep} Image:     {}", r.image);
    println!("{sep} Created:   {}", r.created);
    println!("{sep} Docker:    {}", r.docker_mode);
    println!("{sep} Backend:   {}", r.backend);
    if let Some(ref path) = r.boxfile_path {
        println!("{sep} Boxfile:   {path}");
    }
    if !r.packages.is_empty() {
        println!("{sep} Packages:  {}", r.packages.join(", "));
    }
    if !r.mounts.is_empty() {
        println!("{sep} Mounts:");
        for m in &r.mounts {
            println!("{sep}   {}:{} ({})", m.host, m.guest, m.mode);
        }
    }
}

/// Render doctor result as a human-readable panel.
pub fn render_doctor(result: &crate::core::spec::DoctorResult, ctx: &OutputCtx) {
    let ok_str = if result.ok { "ok" } else { "issues found" };
    if ctx.color() {
        println!("\x1b[1mcbox doctor\x1b[0m — {ok_str}");
    } else {
        println!("cbox doctor — {ok_str}");
    }

    // distrobox
    let dbox = &result.distrobox;
    let present_str = if dbox.present { "yes" } else { "no" };
    let version_str = dbox.version.as_deref().unwrap_or("unknown");
    let supported_str = if dbox.supported { "yes" } else { "no (< 1.6)" };
    println!(
        "  distrobox:  present={present_str}  version={version_str}  supported={supported_str}"
    );

    // backend
    let bk = &result.backend;
    let selected_str = bk.selected.as_deref().unwrap_or("none");
    println!("  backend:    selected={selected_str}");
    if bk.podman.present {
        println!("    podman:   reachable={}", bk.podman.reachable);
    }
    if bk.docker.present {
        println!("    docker:   reachable={}", bk.docker.reachable);
    }

    // keyring (non-fatal informational line)
    let kr = &result.keyring;
    let kr_avail = if kr.available { "yes" } else { "no" };
    println!("  keyring:    available={kr_avail} — {}", kr.detail);

    for w in &result.warnings {
        ctx.warn(w);
    }
}
