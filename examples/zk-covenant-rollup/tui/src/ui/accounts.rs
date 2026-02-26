use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let cov_idx = match app.selected_covenant {
        Some(i) => i,
        None => {
            let msg = Paragraph::new("Select a covenant first (tab 1, then Enter)")
                .block(Block::default().borders(Borders::ALL).title("Accounts"));
            frame.render_widget(msg, area);
            return;
        }
    };

    // Split area: top section for role, bottom for account table
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // ── Role section (single navigable line) ──
    let record = &app.covenants[cov_idx].1;
    let is_deployed = record.deployment_tx_id.is_some();
    let has_deployer = record.deployer_privkey.len() == 32;

    let role_line = if is_deployed {
        // Deployed or imported — show prover
        match app.prover_address() {
            Some(addr) => {
                let bal = app.utxo_tracker.balance(&addr);
                Line::from(format!(" Prover:    {addr}   Balance: {}", format_sompi(bal)))
            }
            None => Line::from(" Prover:    not configured"),
        }
    } else if has_deployer {
        // Undeployed with deployer key
        match app.deployer_address(record) {
            Some(addr) => {
                let bal = app.utxo_tracker.balance(&addr);
                Line::from(format!(" Deployer:  {addr}   Balance: {}", format_sompi(bal)))
            }
            None => Line::from(" Deployer:  N/A"),
        }
    } else {
        Line::from(" No role key available")
    };

    let role_style = if app.role_focused { Style::default().bg(Color::DarkGray) } else { Style::default() };

    let roles_block = Block::default().borders(Borders::ALL).title("Role [y:copy address  j/k:navigate]");
    let roles = Paragraph::new(role_line).style(role_style).block(roles_block);
    frame.render_widget(roles, chunks[0]);

    // ── Accounts table ──
    if app.accounts.is_empty() {
        let msg = Paragraph::new("No accounts yet. Press 'c' to create one.")
            .block(Block::default().borders(Borders::ALL).title("Accounts [c:create  y:copy address  j/k:navigate]"));
        frame.render_widget(msg, chunks[1]);
        return;
    }

    let rows: Vec<Row> = app
        .accounts
        .iter()
        .enumerate()
        .map(|(i, (pubkey, _privkey))| {
            let index = pubkey.as_bytes()[0];
            let addr = app.pubkey_to_address(pubkey).unwrap_or_else(|| "???".into());
            let balance = app.utxo_tracker.balance(&addr);
            let bal_str = format_sompi(balance);
            let is_selected = !app.role_focused && i == app.account_list_index;

            let style = if is_selected { Style::default().bg(Color::DarkGray) } else { Style::default() };
            // Show "*" in the first column for the currently selected account.
            let sel_cell = Cell::from(if is_selected { "*" } else { " " })
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

            Row::new(vec![sel_cell, Cell::from(format!("0x{index:02x}")), Cell::from(addr), Cell::from(bal_str)]).style(style)
        })
        .collect();

    let widths = [Constraint::Length(2), Constraint::Length(6), Constraint::Min(40), Constraint::Length(18)];
    let header = Row::new(vec![" ", "Index", "Address", "L1 Balance"]).style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Accounts [c:create  y:copy address  j/k:navigate]"));

    frame.render_widget(table, chunks[1]);
}

/// Format sompi amount as KAS with 8 decimal places.
pub fn format_sompi(sompi: u64) -> String {
    let kas = sompi / 100_000_000;
    let frac = sompi % 100_000_000;
    if frac == 0 {
        format!("{kas} KAS")
    } else {
        // Trim trailing zeros
        let frac_str = format!("{frac:08}");
        let trimmed = frac_str.trim_end_matches('0');
        format!("{kas}.{trimmed} KAS")
    }
}
