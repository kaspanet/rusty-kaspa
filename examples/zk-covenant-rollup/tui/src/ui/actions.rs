use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.selected_covenant.is_none() {
        let msg = Paragraph::new("Select a covenant first (tab 1, then Enter)")
            .block(Block::default().borders(Borders::ALL).title("Actions"));
        frame.render_widget(msg, area);
        return;
    }

    if app.accounts.is_empty() {
        let msg = Paragraph::new("Create at least one account first (tab 2, then 'c')")
            .block(Block::default().borders(Borders::ALL).title("Actions"));
        frame.render_widget(msg, area);
        return;
    }

    // Build dynamic descriptions showing L1/L2 balances
    let (selected_pk, _) = app.accounts[app.account_list_index];
    let addr_str = app.pubkey_to_address(&selected_pk).unwrap_or_default();
    let l1_balance = app.utxo_tracker.balance(&addr_str);
    let l2_balance = app
        .prover
        .as_ref()
        .map(|p| {
            let w = zk_covenant_rollup_host::mock_chain::from_bytes(selected_pk.as_bytes());
            p.smt.get(&w).unwrap_or(0)
        })
        .unwrap_or(0);

    let actions: Vec<(&str, String)> = vec![
        ("e", format!("Entry (Deposit) — L1: {} sompi available", l1_balance)),
        ("t", {
            if app.accounts.len() >= 2 {
                let src_idx = app.action_src_idx.min(app.accounts.len() - 1);
                let dst_idx = app.action_dst_idx.min(app.accounts.len() - 1);
                let (src_pk, _) = app.accounts[src_idx];
                let (dst_pk, _) = app.accounts[dst_idx];
                let src_l2 = app
                    .prover
                    .as_ref()
                    .map(|p| {
                        let w = zk_covenant_rollup_host::mock_chain::from_bytes(src_pk.as_bytes());
                        p.smt.get(&w).unwrap_or(0)
                    })
                    .unwrap_or(0);
                format!(
                    "Transfer — from idx=0x{:02x} (L2: {}) → to idx=0x{:02x}",
                    src_pk.as_bytes()[0],
                    src_l2,
                    dst_pk.as_bytes()[0],
                )
            } else {
                "Transfer — need 2+ accounts".into()
            }
        }),
        ("x", format!("Exit (Withdrawal) — L2: {} units", l2_balance)),
    ];

    let rows: Vec<Row> = actions
        .iter()
        .enumerate()
        .map(|(i, (key, desc))| {
            let style = if i == app.action_menu_index { Style::default().bg(Color::DarkGray) } else { Style::default() };
            Row::new(vec![Cell::from(format!("[{key}]")).style(Style::default().fg(Color::Yellow)), Cell::from(desc.as_str())])
                .style(style)
        })
        .collect();

    let widths = [Constraint::Length(5), Constraint::Min(50)];
    let header = Row::new(vec!["Key", "Action"]).style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Actions [e:entry  t:transfer  x:exit  Enter:select  j/k:navigate]"));

    frame.render_widget(table, area);
}
