use kaspa_hashes::Hash;
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
    // Get on-chain covenant ID for display.
    let covenant_id_label = app
        .selected_covenant
        .and_then(|i| app.covenants.get(i))
        .map(|(_, rec)| {
            rec.on_chain_covenant_id
                .map(|h| {
                    let s = h.to_string();
                    format!("{}..{}", &s[..8], &s[s.len() - 8..])
                })
                .unwrap_or_else(|| "Undeployed".into())
        })
        .unwrap_or_else(|| "none".into());

    // Load proven state from DB for the selected covenant.
    let proven_state = app
        .selected_covenant
        .and_then(|i| app.covenants.get(i))
        .and_then(|(id, _)| app.db.get_proving_state(*id).ok().flatten());

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // Current L2 state
            Constraint::Length(7),  // Proven L2 state
            Constraint::Min(0),     // Account balances
        ])
        .split(area);

    // ── Current L2 state ──────────────────────────────────────────────────────
    let root_hex = faster_hex::hex_string(bytemuck::bytes_of(&prover.state_root));
    let current_lines = vec![
        Line::from(format!("State root:      {}..{}", &root_hex[..8], &root_hex[root_hex.len() - 8..])),
        Line::from(format!("Seq commitment:  {}", prover.seq_commitment)),
        Line::from(format!("Last block:      {}", prover.last_processed_block)),
        Line::from(format!("Exit leaves:     {}", prover.perm_builder.leaf_count())),
        Line::from(format!("Covenant:        {covenant_id_label}")),
    ];
    let current_block = Block::default().borders(Borders::ALL).title("Current L2 State  r:refetch");
    frame.render_widget(Paragraph::new(current_lines).block(current_block), chunks[0]);

    // ── Proven L2 state (from DB) ─────────────────────────────────────────────
    let proven_lines = if let Some(ps) = proven_state {
        let root_str = ps.state_root.to_string();
        vec![
            Line::from(format!("State root:      {}..{}", &root_str[..8], &root_str[root_str.len() - 8..])),
            Line::from(format!("Seq commitment:  {}", ps.seq_commitment)),
            Line::from(format!("Last proved:     {}", ps.last_proved_block_hash)),
            Line::from(format!("Proof count:     {}", ps.proof_count)),
        ]
    } else {
        vec![Line::styled("No proof submitted yet", Style::default().fg(Color::DarkGray))]
    };
    let proven_block = Block::default().borders(Borders::ALL).title("Proven L2 State");
    frame.render_widget(Paragraph::new(proven_lines).block(proven_block), chunks[1]);

    // ── Account balances from SMT ─────────────────────────────────────────────
    let mut rows = Vec::new();
    for idx in 0u16..=255 {
        if let Some((pk_words, balance)) = prover.smt.get_by_index(idx as u8) {
            let pk_bytes: [u8; 32] = bytemuck::cast(pk_words);
            let pk_hash = Hash::from_bytes(pk_bytes);
            let pk_hex = pk_hash.to_string();
            let owned = app.accounts.iter().any(|(pk, _)| *pk == pk_hash);
            let label = if owned { "owned" } else { "watch" };

            let style = if !owned { Style::default().fg(Color::DarkGray) } else { Style::default() };

            rows.push(
                Row::new(vec![
                    format!("0x{idx:02x}"),
                    format!("{}..{}", &pk_hex[..8], &pk_hex[pk_hex.len() - 8..]),
                    format!("{balance}"),
                    label.to_string(),
                ])
                .style(style),
            );
        }
    }

    let widths = [Constraint::Length(6), Constraint::Length(20), Constraint::Min(15), Constraint::Length(7)];
    let header = Row::new(vec!["Idx", "Pubkey", "L2 Balance", "Type"]).style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("L2 Balances (derived from accepted txs)"));

    frame.render_widget(table, chunks[2]);
}
