use ratatui::layout::Constraint;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, TxStatus};

pub fn draw(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.tx_history.is_empty() {
        let msg = Paragraph::new("No transactions yet")
            .block(Block::default().borders(Borders::ALL).title("Tx History [j/k:navigate  c:copy ID  Enter/o:open in browser]"));
        frame.render_widget(msg, area);
        return;
    }

    let rows: Vec<Row> = app
        .tx_history
        .iter()
        .enumerate()
        .map(|(i, record)| {
            let style = if i == app.tx_history_index { Style::default().bg(Color::DarkGray) } else { Style::default() };
            let tx_str = record.tx_id.to_string();
            let tx_short = if tx_str.len() > 16 { format!("{}..{}", &tx_str[..8], &tx_str[tx_str.len() - 8..]) } else { tx_str };
            let (status_str, status_color) = match &record.status {
                TxStatus::Submitted => ("Submitted".to_string(), Color::Yellow),
                TxStatus::Confirmed => ("Confirmed".to_string(), Color::Green),
                TxStatus::Failed(msg) => (format!("Failed: {msg}"), Color::Red),
            };
            Row::new(vec![
                Cell::from(format!("{}", i + 1)),
                Cell::from(record.action.as_str()),
                Cell::from(format!("{}", record.amount)),
                Cell::from(tx_short),
                Cell::from(status_str).style(Style::default().fg(status_color)),
            ])
            .style(style)
        })
        .collect();

    let widths = [Constraint::Length(4), Constraint::Length(18), Constraint::Length(15), Constraint::Length(20), Constraint::Min(15)];
    let header = Row::new(vec!["#", "Action", "Amount", "Tx ID", "Status"]).style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Tx History [j/k:navigate  c:copy ID  Enter/o:open in browser]"));

    frame.render_widget(table, area);
}
