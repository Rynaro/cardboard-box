//! Pure render function: `view(&Model, &mut Frame)`.
//! No mutation of Model except ratatui scroll bookkeeping (kept in Model).
//! Not unit-tested (smoke/manual only).

#[cfg(feature = "tui")]
mod inner {
    use ratatui::{
        layout::{Alignment, Constraint, Direction, Layout},
        style::{Modifier, Style},
        text::{Line, Span},
        widgets::{
            Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, Wrap,
        },
        Frame,
    };

    use crate::tui::action::{palette_actions, Action, BULK_ACTIONS};
    use crate::tui::bulk::BulkOp;
    use crate::tui::keymap::{keymap_for, KeyContext};
    use crate::tui::model::{Model, Overlay, Screen, StatusLine, WizardStep};
    use crate::tui::strings;
    use crate::tui::theme::{badge_span, header_should_collapse, ok_glyph, Theme};

    const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

    pub fn view(model: &Model, frame: &mut Frame) {
        let area = frame.area();
        // Build theme once per frame from the model's skin + detected color mode.
        let theme = Theme::resolve(model.skin, model.color_mode);

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

            // Render toasts over the list screen.
            render_toasts(model, frame, chunks[1], &theme);
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
            render_toasts(model, frame, chunks[0], &theme);
        }

        // Bulk confirm modal rendered over everything (before overlays).
        if model.bulk_confirm.is_some() {
            render_bulk_confirm(model, frame, area, &theme);
        }

        // Overlays rendered on top of everything.
        match &model.overlay {
            Overlay::None => {}
            Overlay::Cheatsheet => {
                render_cheatsheet(model, frame, area, &theme);
            }
            Overlay::CommandLog { scroll } => {
                render_command_log(model, frame, area, *scroll, &theme);
            }
            Overlay::Palette {
                query,
                matches,
                cursor,
                bulk_only,
            } => {
                render_palette(frame, area, query, matches, *cursor, *bulk_only, &theme);
            }
        }

        // Filter input bar rendered when filter is active.
        if model.filter.is_some() {
            render_filter_bar(model, frame, area, &theme);
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
        // Build title: show filter query hint when active.
        let title_text = if let Some(ref f) = model.filter {
            if f.query.is_empty() {
                " your boxes [/] ".to_string()
            } else {
                format!(" your boxes [/{}_] ", f.query)
            }
        } else {
            " your boxes ".to_string()
        };

        let block = Block::default()
            .title(Span::styled(title_text, theme.title))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(if model.filter.is_some() {
                theme.accent
            } else {
                theme.border
            });

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

        // When a filter is active, only render the matched subset.
        let indices = model.filtered_indices();

        let rows: Vec<Row> = indices
            .iter()
            .map(|&i| {
                let b = &model.boxes[i];
                let is_selected = model.selected == Some(i);
                let row_style = if is_selected {
                    theme.selection
                } else {
                    Style::default()
                };

                let status_span = badge_span(&b.status, theme);
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

    // ─── Filter bar ───────────────────────────────────────────────────────────

    fn render_filter_bar(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        let f = match &model.filter {
            Some(f) => f,
            None => return,
        };

        let match_count = f.matches.len();
        let total = model.boxes.len();
        let hint = format!(
            " / {} ({}/{})  ↑↓ move · enter keep · esc clear ",
            f.query, match_count, total
        );

        // Position the filter bar at the bottom of the area.
        let bar_area = ratatui::layout::Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(2),
            width: area.width,
            height: 1,
        };

        let p = Paragraph::new(Span::styled(hint, theme.accent));
        frame.render_widget(Clear, bar_area);
        frame.render_widget(p, bar_area);
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

        // Sparklines region (only when stats_history has samples).
        let show_sparklines = model
            .stats_history
            .as_ref()
            .map(|h| !h.cpu.is_empty())
            .unwrap_or(false);

        if show_sparklines {
            // Split area: top = detail paragraph, bottom = sparklines.
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(6)])
                .split(area);

            let p = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false });
            frame.render_widget(p, chunks[0]);

            render_sparklines(model, frame, chunks[1], theme);
        } else {
            let p = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false });
            frame.render_widget(p, area);
        }
    }

    // ─── Sparklines (Bundle 2) ────────────────────────────────────────────────

    fn render_sparklines(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        let history = match &model.stats_history {
            Some(h) => h,
            None => return,
        };

        // Split into two rows: CPU on top, mem on bottom.
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(area);

        // CPU sparkline.
        let cpu_data: Vec<u64> = history.cpu.iter().copied().collect();
        let cpu_label = if let Some(&last) = history.cpu.back() {
            format!(" CPU  {:.1}% ", last as f64 / 100.0)
        } else {
            " CPU ".to_string()
        };
        let cpu_sparkline = Sparkline::default()
            .block(
                Block::default()
                    .title(Span::styled(cpu_label, theme.header_cell))
                    .borders(Borders::LEFT | Borders::TOP | Borders::RIGHT)
                    .border_style(theme.border),
            )
            .data(&cpu_data)
            .max(10000) // CPU% × 100
            .style(theme.accent);
        frame.render_widget(cpu_sparkline, rows[0]);

        // Mem sparkline.
        let mem_data: Vec<u64> = history.mem_used.iter().copied().collect();
        let mem_label = if let Some(&used) = history.mem_used.back() {
            let limit = history.mem_limit;
            format!(" mem  {} / {} ", format_bytes(used), format_bytes(limit))
        } else {
            " mem ".to_string()
        };
        let mem_max = if history.mem_limit > 0 {
            history.mem_limit
        } else {
            1024 * 1024 * 1024 // 1 GiB fallback
        };
        let mem_sparkline = Sparkline::default()
            .block(
                Block::default()
                    .title(Span::styled(mem_label, theme.header_cell))
                    .borders(Borders::LEFT | Borders::BOTTOM | Borders::RIGHT)
                    .border_style(theme.border),
            )
            .data(&mem_data)
            .max(mem_max)
            .style(theme.success);
        frame.render_widget(mem_sparkline, rows[1]);
    }

    /// Format bytes as a human-readable string (MiB / GiB).
    fn format_bytes(bytes: u64) -> String {
        const MIB: u64 = 1024 * 1024;
        const GIB: u64 = 1024 * 1024 * 1024;
        if bytes >= GIB {
            format!("{:.1}GiB", bytes as f64 / GIB as f64)
        } else if bytes >= MIB {
            format!("{:.0}MiB", bytes as f64 / MIB as f64)
        } else {
            format!("{bytes}B")
        }
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

    // ─── Cheatsheet overlay ───────────────────────────────────────────────────

    fn render_cheatsheet(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        // Determine the active context for the cheatsheet content.
        let ctx = match model.screen {
            Screen::List => KeyContext::List,
            Screen::Detail => KeyContext::Detail,
            Screen::Wizard => KeyContext::Wizard,
            Screen::ConfirmDestroy => KeyContext::ConfirmDestroy,
            Screen::Progress => KeyContext::Progress,
            Screen::DoctorPanel => KeyContext::DoctorPanel,
        };
        let bindings = keymap_for(ctx);

        let modal_width = 60u16.min(area.width.saturating_sub(4));
        // 3 lines for borders/title/footer + 1 per binding.
        let modal_height = (bindings.len() as u16 + 4).min(area.height.saturating_sub(2));
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = ratatui::layout::Rect {
            x: modal_x,
            y: modal_y,
            width: modal_width,
            height: modal_height,
        };

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(Span::styled(
                " cheatsheet ",
                theme.accent.add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.accent);

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let mut lines: Vec<Line> = bindings
            .iter()
            .map(|kb| {
                Line::from(vec![
                    Span::styled(format!("  {:12}", kb.key), theme.accent),
                    Span::styled("  ", theme.muted),
                    Span::raw(kb.action.label()),
                ])
            })
            .collect();

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("  any key — dismiss", theme.muted)));

        let p = Paragraph::new(lines).wrap(Wrap { trim: false });
        frame.render_widget(p, inner);
    }

    // ─── Command-log overlay ─────────────────────────────────────────────────

    fn render_command_log(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        scroll: usize,
        theme: &Theme,
    ) {
        let modal_width = (area.width.saturating_sub(4)).min(100);
        let modal_height = area.height.saturating_sub(4).max(6);
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = ratatui::layout::Rect {
            x: modal_x,
            y: modal_y,
            width: modal_width,
            height: modal_height,
        };

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(Span::styled(
                strings::CMDLOG_TITLE,
                theme.accent.add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(theme.border);

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        // Read from the shared log (lock briefly for render).
        let entries: Vec<_> = if let Ok(log) = model.cmdlog.lock() {
            log.entries().map(|e| (e.argv.clone(), e.status)).collect()
        } else {
            vec![]
        };

        if entries.is_empty() {
            let p = Paragraph::new(vec![
                Line::from(Span::styled(strings::CMDLOG_EMPTY, theme.muted)),
                Line::from(""),
                Line::from(Span::styled(strings::CMDLOG_HINT, theme.muted)),
            ])
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
            frame.render_widget(p, inner);
            return;
        }

        let mut lines: Vec<Line> = entries
            .iter()
            .map(|(argv, status)| {
                let glyph = match status {
                    Some(0) => Span::styled("✓", theme.success),
                    Some(_) => Span::styled("✗", theme.danger),
                    None => Span::styled("·", theme.muted),
                };
                Line::from(vec![glyph, Span::raw(" "), Span::raw(argv.clone())])
            })
            .collect();

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(strings::CMDLOG_HINT, theme.muted)));

        let p = Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(p, inner);
    }

    // ─── Palette overlay (Bundle 2) ───────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn render_palette(
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        query: &str,
        matches: &[usize],
        cursor: usize,
        bulk_only: bool,
        theme: &Theme,
    ) {
        let source: &[Action] = if bulk_only {
            BULK_ACTIONS
        } else {
            palette_actions()
        };

        let modal_width = 60u16.min(area.width.saturating_sub(4));
        let visible_rows = 10u16;
        let modal_height = (visible_rows + 5).min(area.height.saturating_sub(4));
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = ratatui::layout::Rect {
            x: modal_x,
            y: modal_y,
            width: modal_width,
            height: modal_height,
        };

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(Span::styled(
                strings::PALETTE_TITLE,
                theme.accent.add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.accent);

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        // Split: query bar at top, list below.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Query bar.
        let query_display = format!("> {query}_");
        let query_line = Paragraph::new(Span::styled(query_display, theme.accent));
        frame.render_widget(query_line, chunks[0]);

        // Match list.
        if matches.is_empty() {
            let p = Paragraph::new(Span::styled(strings::PALETTE_NO_MATCH, theme.muted));
            frame.render_widget(p, chunks[1]);
            return;
        }

        let list_area = chunks[1];
        let max_rows = list_area.height as usize;
        // Scroll window around cursor.
        let start = if cursor >= max_rows {
            cursor - max_rows + 1
        } else {
            0
        };

        let mut lines: Vec<Line> = Vec::new();
        for (row_idx, &action_idx) in matches.iter().enumerate().skip(start).take(max_rows) {
            if let Some(&action) = source.get(action_idx) {
                let is_selected = row_idx == cursor;
                let style = if is_selected {
                    theme.selection.add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if is_selected { "> " } else { "  " };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{}", action.label()),
                    style,
                )));
            }
        }

        let p = Paragraph::new(lines);
        frame.render_widget(p, list_area);
    }

    // ─── Bulk confirm modal (Bundle 2) ────────────────────────────────────────

    fn render_bulk_confirm(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        let bulk = match &model.bulk_confirm {
            Some(b) => b,
            None => return,
        };

        let is_dangerous = bulk.op == BulkOp::DestroyUnmanaged;

        let title = match bulk.op {
            BulkOp::PruneStopped => strings::BULK_PRUNE_TITLE,
            BulkOp::StopRunning => strings::BULK_STOP_TITLE,
            BulkOp::DestroyManaged => strings::BULK_DESTROY_MANAGED_TITLE,
            BulkOp::DestroyUnmanaged => strings::BULK_DESTROY_UNMANAGED_TITLE,
        };

        let n = bulk.targets.len();
        let extra_rows = if is_dangerous { 4u16 } else { 2u16 };
        let list_rows = (n as u16).min(8);
        let modal_height = (list_rows + extra_rows + 5).min(area.height.saturating_sub(4));
        let modal_width = 64u16.min(area.width.saturating_sub(4));
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = ratatui::layout::Rect {
            x: modal_x,
            y: modal_y,
            width: modal_width,
            height: modal_height,
        };

        frame.render_widget(Clear, modal_area);

        let border_style = if is_dangerous {
            theme.danger
        } else {
            theme.warning
        };
        let title_style = if is_dangerous {
            theme.danger.add_modifier(Modifier::BOLD)
        } else {
            theme.warning.add_modifier(Modifier::BOLD)
        };

        let block = Block::default()
            .title(Span::styled(title, title_style))
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(border_style);

        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let mut lines: Vec<Line> = Vec::new();

        // Count line.
        lines.push(Line::from(Span::styled(
            format!("{n} boxes:"),
            theme.header_cell,
        )));

        // Target list (capped).
        let visible = bulk.targets.iter().take(8);
        for name in visible {
            lines.push(Line::from(format!("  {name}")));
        }
        if n > 8 {
            lines.push(Line::from(Span::styled(
                format!("  … and {} more", n - 8),
                theme.muted,
            )));
        }

        lines.push(Line::from(""));

        if is_dangerous {
            lines.push(Line::from(Span::styled(
                strings::BULK_UNMANAGED_WARN,
                theme.danger,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "Type {} to confirm:",
                strings::BULK_UNMANAGED_PHRASE
            )));
            lines.push(Line::from(Span::styled(
                format!("> {}_", bulk.typed_confirm),
                theme.accent,
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("esc  cancel", theme.muted)));
        } else {
            lines.push(Line::from(Span::styled(
                "y / enter  confirm   ·   n / esc  cancel",
                theme.muted,
            )));
        }

        let p = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(p, inner);
    }

    // ─── Toast stack ─────────────────────────────────────────────────────────

    fn render_toasts(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect, theme: &Theme) {
        use crate::tui::model::ToastKind;

        if model.toasts.is_empty() {
            return;
        }

        // Render toasts stacked at bottom-right of the given area.
        let max_width = (area.width / 2).clamp(30, 60);
        let toast_height = 1u16;

        for (i, toast) in model.toasts.iter().enumerate() {
            let y_offset = model.toasts.len().saturating_sub(i + 1) as u16;
            let toast_y = area.y + area.height.saturating_sub(y_offset + 1);
            let toast_x = area.x + area.width.saturating_sub(max_width);

            let toast_area = ratatui::layout::Rect {
                x: toast_x,
                y: toast_y,
                width: max_width,
                height: toast_height,
            };

            let style = match toast.kind {
                ToastKind::Success => theme.success,
                ToastKind::Info => theme.accent,
                ToastKind::Error => theme.danger,
            };

            let prefix = match toast.kind {
                ToastKind::Success => "✓ ",
                ToastKind::Info => "· ",
                ToastKind::Error => "✗ ",
            };

            let text = format!("{prefix}{}", toast.text);
            let p = Paragraph::new(Span::styled(text, style));
            frame.render_widget(Clear, toast_area);
            frame.render_widget(p, toast_area);
        }
    }

    // ─── Status bar ───────────────────────────────────────────────────────────

    fn render_status_bar(
        model: &Model,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        theme: &Theme,
    ) {
        // Derive the help string from the keymap (single source of truth, D-1).
        let help = keymap_for(KeyContext::List)
            .iter()
            .map(|kb| format!("{} {}", kb.key, kb.action.label()))
            .collect::<Vec<_>>()
            .join(" · ");

        let (status_text, style) = match &model.status {
            StatusLine::Idle => (help.clone(), theme.muted),
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
