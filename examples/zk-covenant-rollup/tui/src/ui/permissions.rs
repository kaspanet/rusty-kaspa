use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Permission UTXO summary
            Constraint::Min(0),    // Claimable leaves
            Constraint::Length(6), // Pending exits
        ])
        .split(area);

    draw_perm_utxo_summary(frame, app, chunks[0]);
    draw_claimable_leaves(frame, app, chunks[1]);
    draw_pending_exits(frame, app, chunks[2]);
}

fn draw_perm_utxo_summary(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let lines = if let Some(ref perm) = app.perm_utxo {
        let outpoint = format!("{}:{}", perm.utxo.0, perm.utxo.1);
        let total_leaves = perm.exit_data.len();
        vec![
            Line::from(format!("  Outpoint: {outpoint}")),
            Line::from(format!("  Value: {} sompi", perm.value)),
            Line::from(format!("  Claimable leaves: {total_leaves}   Unclaimed: {}", perm.unclaimed)),
        ]
    } else {
        vec![
            Line::from(""),
            Line::styled(
                "  No permission UTXO — submit a proof with exits to see claimable leaves",
                Style::default().fg(Color::DarkGray),
            ),
        ]
    };

    let block = Block::default().borders(Borders::ALL).title(" Permission UTXO ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_claimable_leaves(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if let Some(ref perm) = app.perm_utxo {
        if perm.exit_data.is_empty() {
            let block = Block::default().borders(Borders::ALL).title(" Claimable Leaves [w:withdraw  j/k:navigate] ");
            let msg = Paragraph::new("  All exits claimed").block(block);
            frame.render_widget(msg, area);
            return;
        }

        let rows: Vec<Row> = perm
            .exit_data
            .iter()
            .enumerate()
            .map(|(i, (spk, amount))| {
                let style = if i == app.perm_leaf_index {
                    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if i == app.perm_leaf_index { ">" } else { " " };
                let spk_hex = faster_hex::hex_string(spk);
                let spk_display =
                    if spk_hex.len() > 20 { format!("{}..{}", &spk_hex[..10], &spk_hex[spk_hex.len() - 10..]) } else { spk_hex };
                Row::new(vec![
                    Cell::from(format!("{prefix}{i}")),
                    Cell::from(spk_display),
                    Cell::from(format!("{amount}")),
                    Cell::from("Unclaimed"),
                ])
                .style(style)
            })
            .collect();

        let widths = [Constraint::Length(4), Constraint::Min(30), Constraint::Length(15), Constraint::Length(12)];
        let header = Row::new(vec!["#", "Destination SPK", "Amount", "Status"])
            .style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow));

        let table = Table::new(rows, widths)
            .header(header)
            .block(Block::default().borders(Borders::ALL).title(" Claimable Leaves [w:withdraw  j/k:navigate] "));

        frame.render_widget(table, area);
    } else {
        let block = Block::default().borders(Borders::ALL).title(" Claimable Leaves ");
        let msg = Paragraph::new("  No permission UTXO available").block(block);
        frame.render_widget(msg, area);
    }
}

fn draw_pending_exits(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let lines = if let Some(ref prover) = app.prover {
        if prover.accumulated_exit_data.is_empty() {
            vec![Line::styled("  No pending exits", Style::default().fg(Color::DarkGray))]
        } else {
            let mut lines =
                vec![Line::from(format!("  Accumulated exits: {} (will be in next proof)", prover.accumulated_exit_data.len()))];
            for (spk, amount) in &prover.accumulated_exit_data {
                let spk_hex = faster_hex::hex_string(spk);
                let spk_short =
                    if spk_hex.len() > 20 { format!("{}..{}", &spk_hex[..10], &spk_hex[spk_hex.len() - 10..]) } else { spk_hex };
                lines.push(Line::from(format!("    {spk_short}  →  {amount} L2 units")));
            }
            lines
        }
    } else {
        vec![Line::styled("  Prover not initialized", Style::default().fg(Color::DarkGray))]
    };

    let block = Block::default().borders(Borders::ALL).title(" Pending Exits (awaiting proof) ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
