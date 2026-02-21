use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
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
        Tab::TxHistory => crate::ui::tx_history::draw(frame, app, chunks[1]),
        Tab::Log => draw_log(frame, app, chunks[1]),
    }

    // Popup overlay (if input mode is active)
    if !app.input_mode.is_normal() {
        draw_popup(frame, app);
    }

    // Status bar
    let cov_label = app
        .selected_covenant
        .and_then(|i| app.covenants.get(i))
        .map(|(id, _)| {
            let s = id.to_string();
            s[..8.min(s.len())].to_string()
        })
        .unwrap_or_else(|| "none".into());

    let status = format!(
        " {} | DAA: {} | Covenant: {} | q:quit",
        if app.connected { "Connected" } else { "Disconnected" },
        app.daa_score,
        cov_label,
    );
    let status_bar = Line::from(status).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status_bar, chunks[2]);
}

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let visible_height = area.height.saturating_sub(2) as usize; // borders
    let start = app.log_messages.len().saturating_sub(visible_height);
    let lines: Vec<Line> = app.log_messages[start..].iter().map(|m| Line::from(m.as_str())).collect();
    let log = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Log"));
    frame.render_widget(log, area);
}

/// Centered popup rect of given width/height within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn draw_popup(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 12, frame.area());
    frame.render_widget(Clear, area);

    match &app.input_mode {
        InputMode::Normal => {}
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
