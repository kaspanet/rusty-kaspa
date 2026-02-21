use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Controls
            Constraint::Length(9), // Chain state
            Constraint::Min(0),    // Proof status
        ])
        .split(area);

    draw_controls(frame, app, chunks[0]);
    draw_chain_state(frame, app, chunks[1]);
    draw_proof_status(frame, app, chunks[2]);
}

fn draw_controls(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let lines = vec![
        Line::from(vec![
            ratatui::text::Span::styled("b", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw(format!(":backend [{}]  ", app.prover_backend.label())),
            ratatui::text::Span::styled("k", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw(format!(":kind [{}]  ", app.proof_kind.label())),
            ratatui::text::Span::styled("p", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw(":sync chain  "),
            ratatui::text::Span::styled("r", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ratatui::text::Span::raw(":PROVE"),
        ]),
        Line::from(""),
        Line::from(format!("Sync status: {}", app.proving_status)),
    ];

    let block = Block::default().borders(Borders::ALL).title("Proving Controls");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_chain_state(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut lines = Vec::new();

    if let Some(prover) = &app.prover {
        let root_hex = faster_hex::hex_string(bytemuck::bytes_of(&prover.state_root));
        lines.push(Line::from(format!("State root:       {}..{}", &root_hex[..8], &root_hex[root_hex.len() - 8..])));
        lines.push(Line::from(format!("Seq commitment:   {}", prover.seq_commitment)));
        lines.push(Line::from(format!("Last block:       {}", prover.last_processed_block)));
        lines.push(Line::from(format!("Accumulated:      {} blocks (since last proof)", prover.accumulated_blocks())));

        let total_txs: usize = prover.last_block_txs.iter().map(|b| b.len()).sum();
        lines.push(Line::from(format!("Last batch:       {} blocks, {} txs", prover.last_block_txs.len(), total_txs)));
        lines.push(Line::from(format!("Exit leaves:      {}", prover.perm_builder.leaf_count())));
    } else {
        lines.push(Line::styled("Prover not initialized (select a deployed covenant)", Style::default().fg(Color::DarkGray)));
    }

    let block = Block::default().borders(Borders::ALL).title("Chain State");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_proof_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut lines = Vec::new();

    if app.proof_in_progress {
        lines.push(Line::styled("Proving in progress...", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
        lines.push(Line::styled("(this may take several minutes)", Style::default().fg(Color::DarkGray)));
    } else if let Some(result) = &app.last_proof_result {
        let color = if result.starts_with("Proof failed") { Color::Red } else { Color::Green };
        lines.push(Line::styled(result.clone(), Style::default().fg(color)));
    } else {
        lines.push(Line::styled("No proof generated yet", Style::default().fg(Color::DarkGray)));
        lines.push(Line::from("Press 'r' to start proving accumulated blocks"));
    }

    // Show saved proving state from DB
    if let Some(cov_idx) = app.selected_covenant {
        let covenant_id = app.covenants[cov_idx].0;
        if let Ok(Some(state)) = app.db.get_proving_state(covenant_id) {
            lines.push(Line::from(""));
            lines.push(Line::styled("Last saved proof:", Style::default().add_modifier(Modifier::BOLD)));
            lines.push(Line::from(format!("  Block:  {}", state.last_proved_block_hash)));
            lines.push(Line::from(format!("  Root:   {}", state.state_root)));
            lines.push(Line::from(format!("  Seq:    {}", state.seq_commitment)));
            lines.push(Line::from(format!("  Count:  {}", state.proof_count)));
        }
    }

    let block = Block::default().borders(Borders::ALL).title("Proof Status");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
