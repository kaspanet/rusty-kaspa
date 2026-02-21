use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if let Some(prover) = &app.prover {
        draw_state(frame, app, prover, area);
    } else {
        let msg = Paragraph::new("No L2 state — select a deployed covenant (auto-syncs from VCC v2)")
            .block(Block::default().borders(Borders::ALL).title("Live L2 State (unproven)  r:refetch"));
        frame.render_widget(msg, area);
    }
}

fn draw_state(frame: &mut Frame, app: &App, prover: &crate::prover::RollupProver, area: ratatui::layout::Rect) {
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(0)])
        .split(area);

    // Info section
    let root_hex = faster_hex::hex_string(bytemuck::bytes_of(&prover.state_root));
    let info_lines = vec![
        Line::from(format!("State root:      {root_hex}")),
        Line::from(format!("Seq commitment:  {}", prover.seq_commitment)),
        Line::from(format!("Last block:      {}", prover.last_processed_block)),
        Line::from(format!("Exit leaves:     {}", prover.perm_builder.leaf_count())),
        Line::from(format!(
            "Covenant:        {}",
            app.selected_covenant.and_then(|i| app.covenants.get(i)).map(|(id, _)| id.to_string()).unwrap_or_else(|| "none".into())
        )),
    ];
    let info = Paragraph::new(info_lines).block(Block::default().borders(Borders::ALL).title("Chain State  r:refetch"));
    frame.render_widget(info, chunks[0]);

    // Account balances from SMT
    let mut rows = Vec::new();
    for (pubkey, _privkey) in &app.accounts {
        let pk_words = zk_covenant_rollup_host::mock_chain::from_bytes(pubkey.as_bytes());
        let index = pubkey.as_bytes()[0];
        let balance = prover.smt.get(&pk_words).unwrap_or(0);
        let pk_hex = pubkey.to_string();

        rows.push(Row::new(vec![
            format!("0x{index:02x}"),
            format!("{}..{}", &pk_hex[..8], &pk_hex[pk_hex.len() - 8..]),
            format!("{balance}"),
        ]));
    }

    let widths = [Constraint::Length(6), Constraint::Length(20), Constraint::Min(15)];
    let header = Row::new(vec!["Idx", "Pubkey", "L2 Balance"]).style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("L2 Balances (derived from accepted txs)"));

    frame.render_widget(table, chunks[1]);
}
