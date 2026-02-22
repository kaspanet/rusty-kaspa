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

    // Split area: top section for special roles, bottom for account table
    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    // ── Special Roles section ──
    let deployer_line = match app.deployer_address(&app.covenants[cov_idx].1) {
        Some(addr) => {
            let bal = app.utxo_tracker.balance(&addr);
            Line::from(format!(" Deployer:  {addr}   Balance: {}", format_sompi(bal)))
        }
        None => Line::from(" Deployer:  N/A (imported)"),
    };

    let prover_line = match app.prover_address() {
        Some(addr) => {
            let bal = app.utxo_tracker.balance(&addr);
            Line::from(format!(" Prover:    {addr}   Balance: {}", format_sompi(bal)))
        }
        None => Line::from(" Prover:    not configured"),
    };

    let roles_block = Block::default().borders(Borders::ALL).title("Special Roles");
    let roles = Paragraph::new(vec![deployer_line, prover_line]).block(roles_block);
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

            let style = if i == app.account_list_index { Style::default().bg(Color::DarkGray) } else { Style::default() };

            Row::new(vec![Cell::from(format!("0x{index:02x}")), Cell::from(addr), Cell::from(bal_str)]).style(style)
        })
        .collect();

    let widths = [Constraint::Length(6), Constraint::Min(40), Constraint::Length(18)];
    let header = Row::new(vec!["Index", "Address", "L1 Balance"]).style(Style::default().add_modifier(Modifier::BOLD));

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
