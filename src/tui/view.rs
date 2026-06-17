//! Pure render function: `view(&Model, &mut Frame)`.
//! No mutation of Model except ratatui scroll bookkeeping (kept in Model).
//! Not unit-tested (smoke/manual only).

#[cfg(feature = "tui")]
mod inner {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
        Frame,
    };

    use crate::tui::model::{Model, Screen, StatusLine, WizardStep};

    const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    pub fn view(model: &Model, frame: &mut Frame) {
        let area = frame.area();

        // Split: main body + status bar.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(1)])
            .split(area);

        // Render the active screen.
        match model.screen {
            Screen::List => render_list(model, frame, chunks[0]),
            Screen::Detail => render_detail(model, frame, chunks[0]),
            Screen::Wizard => render_wizard(model, frame, chunks[0]),
            Screen::ConfirmDestroy => {
                render_list(model, frame, chunks[0]);
                render_confirm_destroy(model, frame, chunks[0]);
            }
            Screen::Progress => render_progress(model, frame, chunks[0]),
            Screen::DoctorPanel => render_doctor(model, frame, chunks[0]),
        }

        // Status bar always at bottom.
        render_status_bar(model, frame, chunks[1]);
    }

    // ─── Box list ─────────────────────────────────────────────────────────────

    fn render_list(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::default()
            .title("  cbox — your boxes  ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        if model.boxes.is_empty() && !model.busy {
            let msg = Paragraph::new("No boxes yet. Press 'c' to create your first one.")
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("NAME").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("BACKEND").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("STATUS").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("IMAGE").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("DOCKER").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("CBOX?").style(Style::default().add_modifier(Modifier::BOLD)),
        ])
        .style(Style::default().fg(Color::Yellow));

        let rows: Vec<Row> = model
            .boxes
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let is_selected = model.selected == Some(i);
                let style = if is_selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let status_style = if b.status.to_lowercase().contains("running")
                    || b.status.to_lowercase().contains("up")
                {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let cbox_str = if b.cbox_managed { "yes" } else { "no" };

                Row::new(vec![
                    Cell::from(b.name.clone()),
                    Cell::from(b.backend.clone()),
                    Cell::from(b.status.clone()).style(if is_selected {
                        style
                    } else {
                        status_style
                    }),
                    Cell::from(b.image.clone()),
                    Cell::from(b.docker_mode.clone()),
                    Cell::from(cbox_str),
                ])
                .style(style)
            })
            .collect();

        let widths = [
            Constraint::Percentage(20),
            Constraint::Percentage(10),
            Constraint::Percentage(13),
            Constraint::Percentage(37),
            Constraint::Percentage(10),
            Constraint::Percentage(10),
        ];

        let table = Table::new(rows, widths)
            .block(block)
            .header(header)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(table, area);
    }

    // ─── Detail panel ─────────────────────────────────────────────────────────

    fn render_detail(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::default()
            .title("  Box Detail  ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        if model.busy {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let p = Paragraph::new(format!("{spinner} Loading…"))
                .block(block)
                .alignment(Alignment::Center);
            frame.render_widget(p, area);
            return;
        }

        let content = match &model.detail {
            None => {
                let p = Paragraph::new("No detail loaded.")
                    .block(block)
                    .alignment(Alignment::Center);
                frame.render_widget(p, area);
                return;
            }
            Some(d) => d,
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name:     ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.name.clone()),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    content.status.clone(),
                    if content.status.to_lowercase().contains("running") {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]),
            Line::from(vec![
                Span::styled("Image:    ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.image.clone()),
            ]),
            Line::from(vec![
                Span::styled("Created:  ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.created.clone()),
            ]),
            Line::from(vec![
                Span::styled("Docker:   ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.docker_mode.clone()),
            ]),
            Line::from(vec![
                Span::styled("Backend:  ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.backend.clone()),
            ]),
            Line::from(vec![
                Span::styled("ID:       ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.id.clone()),
            ]),
            Line::from(vec![
                Span::styled("Boxfile:  ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(
                    content
                        .boxfile_path
                        .clone()
                        .unwrap_or_else(|| "(none)".to_string()),
                ),
            ]),
        ];

        if !content.packages.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Packages: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(content.packages.join(", ")),
            ]));
        }

        if !content.mounts.is_empty() {
            lines.push(Line::from(Span::styled(
                "Mounts:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for m in &content.mounts {
                lines.push(Line::from(format!(
                    "  {}  →  {}  ({})",
                    m.host, m.guest, m.mode
                )));
            }
        }

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(p, area);
    }

    // ─── Create wizard ────────────────────────────────────────────────────────

    fn render_wizard(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let wizard = match &model.wizard {
            Some(w) => w,
            None => return,
        };

        let steps = &["Name", "Image", "Packages", "Docker mode", "Confirm"];
        let current_step_idx = match wizard.step {
            WizardStep::Name => 0,
            WizardStep::Image => 1,
            WizardStep::Packages => 2,
            WizardStep::DockerMode => 3,
            WizardStep::Confirm => 4,
        };

        let step_indicator: String = steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                if i == current_step_idx {
                    format!("[{s}]")
                } else {
                    s.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" › ");

        let block = Block::default()
            .title(format!("  Create Box — {step_indicator}  "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .margin(1)
            .split(area);

        frame.render_widget(block, area);

        let (label, value) = match wizard.step {
            WizardStep::Name => ("Box name:", wizard.name.clone()),
            WizardStep::Image => ("Image:", wizard.image.clone()),
            WizardStep::Packages => ("Packages (space-separated):", wizard.packages_raw.clone()),
            WizardStep::DockerMode => {
                let opts = ["none", "host", "nested"];
                let display: String = opts
                    .iter()
                    .enumerate()
                    .map(|(i, o)| {
                        if i == wizard.docker_mode_idx {
                            format!("[{o}]")
                        } else {
                            o.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ");
                ("Docker mode:", display)
            }
            WizardStep::Confirm => {
                let pkgs = if wizard.packages_raw.is_empty() {
                    "(none)".to_string()
                } else {
                    wizard.packages_raw.clone()
                };
                let summary = format!(
                    "name: {}\nimage: {}\npackages: {}\ndocker: {}",
                    wizard.name,
                    wizard.image,
                    pkgs,
                    wizard.docker_mode_str()
                );
                ("Ready to create:", summary)
            }
        };

        let field = Paragraph::new(format!("{label}\n{value}"))
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });
        frame.render_widget(field, chunks[1]);

        let hint = Paragraph::new("Tab/Enter: next  |  Shift-Tab: back  |  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, chunks[2]);
    }

    // ─── Confirm destroy modal ────────────────────────────────────────────────

    fn render_confirm_destroy(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let confirm = match &model.confirm {
            Some(c) => c,
            None => return,
        };

        let modal_width = 60u16.min(area.width.saturating_sub(4));
        let modal_height = 7u16;
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = ratatui::layout::Rect {
            x: modal_x,
            y: modal_y,
            width: modal_width,
            height: modal_height,
        };

        frame.render_widget(Clear, modal_area);

        let rm_home_indicator = if confirm.rm_home { "[x]" } else { "[ ]" };
        let text = format!(
            "Destroy \"{}\"?\n\n\
             Its $HOME is preserved unless you also remove it.\n\
             {rm_home_indicator} h: also remove $HOME\n\n\
             [y]es / [n]o",
            confirm.name
        );

        let modal = Paragraph::new(text)
            .block(
                Block::default()
                    .title("  Confirm Destroy  ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            )
            .wrap(Wrap { trim: true });

        frame.render_widget(modal, modal_area);
    }

    // ─── Progress screen ──────────────────────────────────────────────────────

    fn render_progress(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let progress = match &model.progress {
            Some(p) => p,
            None => return,
        };

        let block = Block::default()
            .title(format!("  {}  ", progress.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        // Show recreate confirm if needed.
        if progress.recreate_confirm {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let msg = progress
                .recreate_msg
                .clone()
                .unwrap_or_else(|| "Recreate needed.".to_string());
            let p = Paragraph::new(format!("{}\n\nRecreate now? [y]es / [n]o", msg))
                .wrap(Wrap { trim: true });
            frame.render_widget(p, inner);
            return;
        }

        if model.busy {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let p = Paragraph::new(format!("{spinner} Running…")).alignment(Alignment::Center);
            frame.render_widget(p, inner);
            return;
        }

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if progress.steps.is_empty() {
            let p =
                Paragraph::new("Done. Press Enter or Esc to return.").alignment(Alignment::Center);
            frame.render_widget(p, inner);
            return;
        }

        let header = Row::new(vec![
            Cell::from("#").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("TYPE").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("STATUS").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("MS").style(Style::default().add_modifier(Modifier::BOLD)),
        ]);

        let rows: Vec<Row> = progress
            .steps
            .iter()
            .map(|s| {
                let status_style = match s.status.as_str() {
                    "ran" | "copied" => Style::default().fg(Color::Green),
                    "skipped" => Style::default().fg(Color::DarkGray),
                    "failed" => Style::default().fg(Color::Red),
                    _ => Style::default(),
                };
                Row::new(vec![
                    Cell::from(s.idx.to_string()),
                    Cell::from(s.step_type.clone()),
                    Cell::from(s.status.clone()).style(status_style),
                    Cell::from(s.duration_ms.to_string()),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(4),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
        ];

        let table = Table::new(rows, widths).header(header);
        frame.render_widget(table, inner);
    }

    // ─── Doctor panel ─────────────────────────────────────────────────────────

    fn render_doctor(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let block = Block::default()
            .title("  Doctor  ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));

        if model.busy {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let p =
                Paragraph::new(format!("{spinner} Running doctor…")).alignment(Alignment::Center);
            frame.render_widget(p, inner);
            return;
        }

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let content = match &model.doctor {
            None => {
                let p = Paragraph::new("No doctor data.").alignment(Alignment::Center);
                frame.render_widget(p, inner);
                return;
            }
            Some(d) => d,
        };

        let ok_str = |b: bool| if b { "✓" } else { "✗" };

        let mut lines = vec![
            Line::from(format!(
                "distrobox present: {}",
                ok_str(content.distrobox.present)
            )),
            Line::from(format!(
                "distrobox version: {}",
                content.distrobox.version.as_deref().unwrap_or("unknown")
            )),
            Line::from(format!(
                "distrobox supported: {}",
                ok_str(content.distrobox.supported)
            )),
            Line::from(""),
            Line::from(format!(
                "backend selected: {}",
                content.backend.selected.as_deref().unwrap_or("(none)")
            )),
            Line::from(format!(
                "podman present: {}  reachable: {}",
                ok_str(content.backend.podman.present),
                ok_str(content.backend.podman.reachable)
            )),
            Line::from(format!(
                "docker present: {}  reachable: {}",
                ok_str(content.backend.docker.present),
                ok_str(content.backend.docker.reachable)
            )),
        ];

        if !content.warnings.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Warnings:",
                Style::default().fg(Color::Yellow),
            )));
            for w in &content.warnings {
                lines.push(Line::from(format!("  ! {w}")));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Esc or q to return.",
            Style::default().fg(Color::DarkGray),
        )));

        let p = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(p, inner);
    }

    // ─── Status bar ───────────────────────────────────────────────────────────

    fn render_status_bar(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
        let help =
            "↑↓ move · enter open · c create · d destroy · a apply · e edit · ? doctor · q quit";

        let (status_text, style) = match &model.status {
            StatusLine::Idle => (help.to_string(), Style::default().fg(Color::DarkGray)),
            StatusLine::Busy(msg) => {
                let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
                (
                    format!("{spinner} {msg}"),
                    Style::default().fg(Color::Yellow),
                )
            }
            StatusLine::Ok(msg) => (
                format!("  {msg}  |  {help}"),
                Style::default().fg(Color::Green),
            ),
            StatusLine::Error(msg) => (format!("error: {msg}"), Style::default().fg(Color::Red)),
        };

        let p = Paragraph::new(status_text).style(style);
        frame.render_widget(p, area);
    }
}

// Public re-export under the feature gate.
#[cfg(feature = "tui")]
pub use inner::view;
