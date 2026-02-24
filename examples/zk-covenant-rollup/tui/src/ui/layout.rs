use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::app::{App, InputMode, Tab};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Content
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Tab bar
    let tab_titles: Vec<&str> = Tab::all().iter().map(|t| t.title()).collect();
    let tabs = Tabs::new(tab_titles)
        .select(app.active_tab.index())
        .block(Block::default().borders(Borders::ALL).title("ZK Covenant Rollup"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, chunks[0]);

    // Content area
    match app.active_tab {
        Tab::Covenants => crate::ui::covenant::draw(frame, app, chunks[1]),
        Tab::Accounts => crate::ui::accounts::draw(frame, app, chunks[1]),
        Tab::Actions => crate::ui::actions::draw(frame, app, chunks[1]),
        Tab::State => crate::ui::state::draw(frame, app, chunks[1]),
        Tab::Proving => crate::ui::prover::draw(frame, app, chunks[1]),
        Tab::Permissions => crate::ui::permissions::draw(frame, app, chunks[1]),
        Tab::TxHistory => crate::ui::tx_history::draw(frame, app, chunks[1]),
        Tab::Log => draw_log(frame, app, chunks[1]),
    }

    // Popup overlay (if input mode is active)
    if !app.input_mode.is_normal() {
        draw_popup(frame, app);
    }

    // Status bar
    let flash_active = app.status_flash.as_ref().map(|(_, t)| t.elapsed() < std::time::Duration::from_secs(2)).unwrap_or(false);

    let status_bar = if flash_active {
        let msg = &app.status_flash.as_ref().unwrap().0;
        let bg = app.status_flash_color;
        Line::from(format!(" {msg}")).style(Style::default().bg(bg).fg(Color::Black))
    } else {
        let cov_label = app
            .selected_covenant
            .and_then(|i| app.covenants.get(i))
            .map(|(id, _)| {
                let s = id.to_string();
                s[..8.min(s.len())].to_string()
            })
            .unwrap_or_else(|| "none".into());

        let conn = if app.connected { "Connected" } else { "Disconnected" };
        let mut spans: Vec<Span> = vec![Span::raw(format!(" {conn} | DAA: {} | Cov: {cov_label}", app.daa_score))];

        // Active background task indicators
        if app.deploy_in_progress {
            spans.push(Span::raw(" "));
            spans.push(Span::styled("[Deploying]", Style::default().fg(Color::Yellow)));
        }
        if app.proof_in_progress {
            spans.push(Span::raw(" "));
            spans.push(Span::styled("[Proving]", Style::default().fg(Color::Cyan)));
        }
        if app.chain_sync_active {
            spans.push(Span::raw(" "));
            spans.push(Span::styled("[Syncing]", Style::default().fg(Color::Blue)));
        }
        if app.pending_submit_count > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("[Submit({})]", app.pending_submit_count), Style::default().fg(Color::Magenta)));
        }

        spans.push(Span::raw(" | Ctrl+L  Ctrl+Q"));

        Line::from(spans).style(Style::default().bg(Color::DarkGray).fg(Color::White))
    };
    frame.render_widget(status_bar, chunks[2]);
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let visible_height = area.height.saturating_sub(2) as usize;
    let total = app.log_messages.len();
    let scroll = if app.log_selected >= app.log_scroll + visible_height {
        app.log_selected.saturating_sub(visible_height.saturating_sub(1))
    } else {
        app.log_scroll.min(app.log_selected)
    };
    let end = (scroll + visible_height).min(total);
    let lines: Vec<Line> = app.log_messages[scroll..end]
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let line = Line::from(m.as_str());
            if scroll + i == app.log_selected {
                line.style(Style::default().bg(Color::DarkGray))
            } else {
                line
            }
        })
        .collect();
    let (scroll_label, toggle_hint) = if app.log_auto_scroll { ("FOLLOWING", "f:pause") } else { ("PAUSED", "f:follow") };
    let title = if total == 0 {
        format!(" Log [0/0] {scroll_label}  {toggle_hint}  j/k:nav  Enter:expand ")
    } else {
        format!(" Log [{}/{}] {scroll_label}  {toggle_hint}  j/k:nav  Enter:expand ", app.log_selected + 1, total)
    };
    frame.render_widget(Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)), area);
}

/// Centered popup rect of given width/height within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn draw_popup(frame: &mut Frame, app: &App) {
    // ViewDetail popup uses the full screen (minus margins)
    if let InputMode::ViewDetail { lines, scroll } = &app.input_mode {
        let area = centered_rect(frame.area().width.saturating_sub(4), frame.area().height.saturating_sub(4), frame.area());
        frame.render_widget(Clear, area);
        let shown: Vec<Line> = lines.iter().map(|l| Line::from(l.as_str())).collect();
        let para = Paragraph::new(shown).wrap(Wrap { trim: false }).scroll((*scroll as u16, 0)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Detail  j/k:scroll  Esc:close "),
        );
        frame.render_widget(para, area);
        return;
    }

    let area = centered_rect(70, 12, frame.area());
    frame.render_widget(Clear, area);

    match &app.input_mode {
        InputMode::Normal => {}
        InputMode::ViewDetail { .. } => unreachable!("handled above"),
        InputMode::PromptAmount { action, buffer, context } => {
            let mut lines: Vec<Line> = context.lines().map(|l| Line::from(l.to_string())).collect();
            lines.push(Line::from(""));
            lines.push(Line::styled(format!("> {buffer}_"), Style::default().fg(Color::White)));
            lines.push(Line::from(""));
            lines.push(Line::styled("Esc: cancel", Style::default().fg(Color::DarkGray)));

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(format!(" {} ", action.label()));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
        InputMode::Confirm { action, summary, .. } => {
            let lines: Vec<Line> = summary.iter().map(|l| Line::from(l.as_str())).collect();

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green))
                .title(format!(" Confirm {} ", action.label()));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
        InputMode::ConfirmDelete { lines, .. } => {
            let is_deployed = lines.iter().any(|l| l.contains("WARNING"));
            let color = if is_deployed { Color::Red } else { Color::Yellow };
            let rendered: Vec<Line> = lines.iter().map(|l| Line::from(l.as_str())).collect();
            let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(color)).title(" Delete Covenant ");
            frame.render_widget(Paragraph::new(rendered).block(block), area);
        }
        InputMode::Processing { action } => {
            let lines = vec![
                Line::from(""),
                Line::styled("Building transaction...", Style::default().fg(Color::Cyan)),
                Line::from(""),
                Line::styled(format!("(nonce grinding for {} prefix)", action.label()), Style::default().fg(Color::DarkGray)),
            ];

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(" {} ", action.label()));
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
    }
}
