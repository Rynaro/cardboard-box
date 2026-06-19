//! Pure render function: `view(&Model, &mut Frame)`.
//! No mutation of Model except ratatui scroll bookkeeping (kept in Model).
//! Not unit-tested (smoke/manual only).

#[cfg(feature = "tui")]
mod inner {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Modifier, Style},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
        Frame,
    };

    use crate::tui::model::{Model, Screen, StatusLine, WizardStep};
    use crate::tui::strings;
    use crate::tui::theme::{badge_span, header_should_collapse, ok_glyph, Theme};

    const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    pub fn view(model: &Model, frame: &mut Frame) {
        let area = frame.area();
        // Build theme once per frame from the model's detected color mode.
        let theme = Theme::resolve(model.color_mode);

        if model.screen == Screen::List {
            // List screen gets a 3-region vertical split: header | body | status.
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(1),
                ])
                .split(area);

            render_brand_header(frame, chunks[0], area.width, &theme);
            render_list(model, frame, chunks[1], &theme);
            render_status_bar(model, frame, chunks[2], &theme);
        } else {
            // All other screens: 2-region split (body | status).
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(area);

            match model.screen {
                Screen::List => unreachable!(),
                Screen::Detail => render_detail(model, frame, chunks[0], &theme),
                Screen::Wizard => render_wizard(model, frame, chunks[0], &theme),
                Screen::ConfirmDestroy => {
                    render_list(model, frame, chunks[0], &theme);
                    render_confirm_destroy(model, frame, chunks[0], &theme);
                }
                Screen::Progress => render_progress(model, frame, chunks[0], &theme),
                Screen::DoctorPanel => render_doctor(model, frame, chunks[0], &theme),
            }

            render_status_bar(model, frame, chunks[1], &theme);
        }
    }

    // ─── Brand header ──────────────────────────────────────────────────────────

    fn render_brand_header(
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        width: u16,
        theme: &Theme,
    ) {
        let line = if header_should_collapse(width) {
            // Narrow: logo + wordmark only.
            Line::from(vec![
                Span::styled(strings::LOGO_GLYPH, theme.brand_logo),
                Span::raw(" "),
                Span::styled(strings::WORDMARK, theme.brand_name),
            ])
        } else {
            // Wide: logo + wordmark + separator + tagline.
            Line::from(vec![
                Span::styled(strings::LOGO_GLYPH, theme.brand_logo),
                Span::raw(" "),
                Span::styled(strings::WORDMARK, theme.brand_name),
                Span::raw(" "),
                Span::styled("·", theme.muted),
                Span::raw(" "),
                Span::styled(strings::TAGLINE, theme.brand_tagline),
            ])
        };
        let p = Paragraph::new(line);
        frame.render_widget(p, area);
    }

    // ─── Box list ─────────────────────────────────────────────────────────────

    fn render_list(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect, theme: &Theme) {
        let block = Block::default()
            .title(Span::styled(" your boxes ", theme.title))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(theme.border);

        if model.busy && model.boxes.is_empty() {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let msg = Paragraph::new(Span::styled(
                format!("{spinner} {}", strings::LOADING_LIST),
                theme.accent,
            ))
            .block(block)
            .alignment(Alignment::Center);
            frame.render_widget(msg, area);
            return;
        }

        if model.boxes.is_empty() && !model.busy {
            // Two-span empty state: sentence + highlighted key.
            let empty_line = Line::from(vec![
                Span::styled("Nothing boxed up yet.  Press  ", theme.muted),
                Span::styled("c", theme.accent),
                Span::styled("  to pack your first one.", theme.muted),
            ]);
            let msg = Paragraph::new(empty_line)
                .block(block)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(msg, area);
            return;
        }

        let header = Row::new(vec![
            Cell::from("NAME").style(theme.header_cell),
            Cell::from("BACKEND").style(theme.header_cell),
            Cell::from("STATUS").style(theme.header_cell),
            Cell::from("IMAGE").style(theme.header_cell),
            Cell::from("DOCKER").style(theme.header_cell),
            Cell::from("CBOX?").style(theme.header_cell),
        ]);

        let rows: Vec<Row> = model
            .boxes
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let is_selected = model.selected == Some(i);
                let row_style = if is_selected {
                    theme.selection
                } else {
                    Style::default()
                };

                let status_span = if is_selected {
                    // On selected row the row_style already drives bg/fg — just use the
                    // badge glyph+label without overriding color so selection stands out.
                    badge_span(&b.status, theme)
                } else {
                    badge_span(&b.status, theme)
                };

                let cbox_str = if b.cbox_managed { "yes" } else { "no" };

                Row::new(vec![
                    Cell::from(b.name.clone()),
                    Cell::from(b.backend.clone()),
                    Cell::from(status_span.content.to_string()).style(if is_selected {
                        row_style
                    } else {
                        status_span.style
                    }),
                    Cell::from(b.image.clone()),
                    Cell::from(b.docker_mode.clone()),
                    Cell::from(cbox_str),
                ])
                .style(row_style)
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

    fn render_detail(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect, theme: &Theme) {
        let block = Block::default()
            .title(Span::styled(" box detail ", theme.title))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(theme.border);

        if model.busy {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let p = Paragraph::new(Span::styled(
                format!("{spinner} {}", strings::LOADING_DETAIL),
                theme.accent,
            ))
            .block(block)
            .alignment(Alignment::Center);
            frame.render_widget(p, area);
            return;
        }

        let content = match &model.detail {
            None => {
                let p = Paragraph::new(Span::styled(strings::EMPTY_DETAIL, theme.muted))
                    .block(block)
                    .alignment(Alignment::Center);
                frame.render_widget(p, area);
                return;
            }
            Some(d) => d,
        };

        let status_span = badge_span(&content.status, theme);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Name:     ", theme.header_cell),
                Span::raw(content.name.clone()),
            ]),
            Line::from(vec![
                Span::styled("Status:   ", theme.header_cell),
                status_span,
            ]),
            Line::from(vec![
                Span::styled("Image:    ", theme.header_cell),
                Span::raw(content.image.clone()),
            ]),
            Line::from(vec![
                Span::styled("Created:  ", theme.header_cell),
                Span::raw(content.created.clone()),
            ]),
            Line::from(vec![
                Span::styled("Docker:   ", theme.header_cell),
                Span::raw(content.docker_mode.clone()),
            ]),
            Line::from(vec![
                Span::styled("Backend:  ", theme.header_cell),
                Span::raw(content.backend.clone()),
            ]),
            Line::from(vec![
                Span::styled("ID:       ", theme.header_cell),
                Span::raw(content.id.clone()),
            ]),
            Line::from(vec![
                Span::styled("Boxfile:  ", theme.header_cell),
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
                Span::styled("Packages: ", theme.header_cell),
                Span::raw(content.packages.join(", ")),
            ]));
        }

        if !content.mounts.is_empty() {
            lines.push(Line::from(Span::styled("Mounts:", theme.header_cell)));
            for m in &content.mounts {
                lines.push(Line::from(vec![
                    Span::raw(format!("  {}  ", m.host)),
                    Span::styled("→", theme.muted),
                    Span::raw(format!("  {}  ({})", m.guest, m.mode)),
                ]));
            }
        }

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(p, area);
    }

    // ─── Create wizard ────────────────────────────────────────────────────────

    fn render_wizard(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect, theme: &Theme) {
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

        // Build step indicator as spans: active step in success style, inactive in muted.
        let mut step_spans: Vec<Span> = Vec::new();
        for (i, s) in steps.iter().enumerate() {
            if i > 0 {
                step_spans.push(Span::styled(" › ", theme.muted));
            }
            if i == current_step_idx {
                step_spans.push(Span::styled(format!("[{s}]"), theme.success));
            } else {
                step_spans.push(Span::styled(s.to_string(), theme.muted));
            }
        }
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
            .title(Span::styled(
                format!("  pack a box — {step_indicator}  "),
                theme.title,
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(theme.accent);

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

        // Render label in header_cell style; value in default fg.
        let field_line = Line::from(vec![
            Span::styled(format!("{label}\n"), theme.header_cell),
            Span::raw(value),
        ]);
        let field = Paragraph::new(field_line).wrap(Wrap { trim: false });
        frame.render_widget(field, chunks[1]);

        let hint = Paragraph::new(Span::styled(
            "Tab/Enter: next  |  Shift-Tab: back  |  Esc: cancel",
            theme.muted,
        ));
        frame.render_widget(hint, chunks[2]);
    }

    // ─── Confirm destroy modal ────────────────────────────────────────────────

    fn render_confirm_destroy(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
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

        // Build styled content lines.
        let danger_title = Span::styled(
            " confirm destroy ",
            theme.danger.add_modifier(Modifier::BOLD),
        );
        let block = Block::default()
            .title(danger_title)
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.danger);

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let content = vec![
            Line::from(format!("Destroy \"{}\"?", confirm.name)),
            Line::from(""),
            Line::from("Its $HOME is preserved unless you also remove it."),
            Line::from(Span::styled(
                format!("{rm_home_indicator} h: also remove $HOME"),
                theme.warning,
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("[y]", theme.danger),
                Span::raw("es / "),
                Span::styled("[n]", theme.success),
                Span::raw("o"),
            ]),
        ];
        let modal = Paragraph::new(content).wrap(Wrap { trim: true });
        frame.render_widget(modal, inner);
    }

    // ─── Progress screen ──────────────────────────────────────────────────────

    fn render_progress(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        let progress = match &model.progress {
            Some(p) => p,
            None => return,
        };

        let block = Block::default()
            .title(Span::styled(format!("  {}  ", progress.title), theme.title))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(theme.accent);

        // Show recreate confirm if needed.
        if progress.recreate_confirm {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let msg = progress
                .recreate_msg
                .clone()
                .unwrap_or_else(|| "Recreate needed.".to_string());
            let content = vec![
                Line::from(msg),
                Line::from(""),
                Line::from(vec![
                    Span::raw("Recreate now? "),
                    Span::styled("[y]", theme.danger),
                    Span::raw("es / "),
                    Span::styled("[n]", theme.success),
                    Span::raw("o"),
                ]),
            ];
            let p = Paragraph::new(content).wrap(Wrap { trim: true });
            frame.render_widget(p, inner);
            return;
        }

        if model.busy {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let p = Paragraph::new(Span::styled(
                format!("{spinner} {}", strings::PROGRESS_RUNNING),
                theme.accent,
            ))
            .alignment(Alignment::Center);
            frame.render_widget(p, inner);
            return;
        }

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if progress.steps.is_empty() {
            let p = Paragraph::new(Span::styled(strings::PROGRESS_DONE, theme.muted))
                .alignment(Alignment::Center);
            frame.render_widget(p, inner);
            return;
        }

        let header = Row::new(vec![
            Cell::from("#").style(theme.header_cell),
            Cell::from("TYPE").style(theme.header_cell),
            Cell::from("STATUS").style(theme.header_cell),
            Cell::from("MS").style(theme.header_cell),
        ]);

        let rows: Vec<Row> = progress
            .steps
            .iter()
            .map(|s| {
                let status_style = match s.status.as_str() {
                    "ran" | "copied" => theme.success,
                    "skipped" => theme.muted,
                    "failed" => theme.danger,
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

    fn render_doctor(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect, theme: &Theme) {
        let block = Block::default()
            .title(Span::styled(" doctor ", theme.title))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(theme.warning);

        if model.busy {
            let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let p = Paragraph::new(Span::styled(
                format!("{spinner} {}", strings::LOADING_DOCTOR),
                theme.warning,
            ))
            .alignment(Alignment::Center);
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

        let mut lines = vec![
            Line::from(vec![
                Span::raw("distrobox present: "),
                ok_glyph(content.distrobox.present, theme),
            ]),
            Line::from(format!(
                "distrobox version: {}",
                content.distrobox.version.as_deref().unwrap_or("unknown")
            )),
            Line::from(vec![
                Span::raw("distrobox supported: "),
                ok_glyph(content.distrobox.supported, theme),
            ]),
            Line::from(""),
            Line::from(format!(
                "backend selected: {}",
                content.backend.selected.as_deref().unwrap_or("(none)")
            )),
            Line::from(vec![
                Span::raw("podman present: "),
                ok_glyph(content.backend.podman.present, theme),
                Span::raw("  reachable: "),
                ok_glyph(content.backend.podman.reachable, theme),
            ]),
            Line::from(vec![
                Span::raw("docker present: "),
                ok_glyph(content.backend.docker.present, theme),
                Span::raw("  reachable: "),
                ok_glyph(content.backend.docker.reachable, theme),
            ]),
        ];

        if !content.warnings.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Warnings:", theme.warning)));
            for w in &content.warnings {
                lines.push(Line::from(vec![
                    Span::styled("! ", theme.warning),
                    Span::raw(w.clone()),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Esc or q to return.",
            theme.muted,
        )));

        let p = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(p, inner);
    }

    // ─── Status bar ───────────────────────────────────────────────────────────

    fn render_status_bar(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        let help = strings::HELP;

        let (status_text, style) = match &model.status {
            StatusLine::Idle => (help.to_string(), theme.muted),
            StatusLine::Busy(msg) => {
                let spinner = SPINNER_FRAMES[model.spinner_tick % SPINNER_FRAMES.len()];
                (format!("{spinner} {msg}"), theme.accent)
            }
            StatusLine::Ok(msg) => (format!("  {msg}  ·  {help}"), theme.success),
            StatusLine::Error(msg) => (format!("{}{}", strings::ERROR_PREFIX, msg), theme.danger),
        };

        let p = Paragraph::new(Span::styled(status_text, style));
        frame.render_widget(p, area);
    }
}

// Public re-export under the feature gate.
#[cfg(feature = "tui")]
pub use inner::view;
