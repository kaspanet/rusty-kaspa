use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.covenants.is_empty() {
        let msg = Paragraph::new("No covenants yet. Press 'c' to create one, or 'i' to import.")
            .block(Block::default().borders(Borders::ALL).title("Covenants"));
        frame.render_widget(msg, area);
        return;
    }

    let rows: Vec<Row> = app
        .covenants
        .iter()
        .enumerate()
        .map(|(i, (_, rec))| {
            // Show on-chain covenant ID if deployed, otherwise "Undeployed".
            let id_label = rec
                .on_chain_covenant_id
                .map(|h| {
                    let s = h.to_string();
                    format!("{}...", &s[..16.min(s.len())])
                })
                .unwrap_or_else(|| "Undeployed".into());
            let deployed = if rec.deployment_tx_id.is_some() { "Yes" } else { "No" };
            let kind = if rec.proof_kind == 1 { "Groth16" } else { "Succinct" };
            let origin = if rec.deployer_privkey.len() == 32 { "Created" } else { "Imported" };
            let addr = app.deployer_address(rec).unwrap_or_else(|| "N/A".into());
            let selected_marker = if app.selected_covenant == Some(i) { "*" } else { " " };

            let style = if i == app.covenant_list_index { Style::default().bg(Color::DarkGray) } else { Style::default() };

            Row::new(vec![
                Cell::from(selected_marker.to_string()),
                Cell::from(id_label),
                Cell::from(deployed.to_string()),
                Cell::from(kind.to_string()),
                Cell::from(origin.to_string()),
                Cell::from(addr),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(1),
        Constraint::Length(20),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(40),
    ];
    let header = Row::new(vec![" ", "Covenant ID", "Deployed", "Kind", "Origin", "Deployer Address"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Covenants [c:create  i:import  d:deploy  t:proof-type  y:export  x:delete  Enter:select  j/k:navigate]"),
    );

    frame.render_widget(table, area);
}
