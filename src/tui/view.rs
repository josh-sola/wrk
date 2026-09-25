use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use super::state::{App, Screen};

const HELP: &str = "\u{2191}/\u{2193} or ^P/^N move  Enter select  Esc back  ^C quit";

pub fn render(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Tree => render_picker(
            frame,
            "Tree",
            &app.tree_filter,
            &tree_rows(app),
            app.tree_selected,
        ),
        Screen::NewRepo if app.repos.is_empty() => render_message(
            frame,
            "New tree",
            "No repos found. Run `wrk clone <url>` first.",
        ),
        Screen::NewRepo => render_picker(
            frame,
            "New tree: pick a repo",
            &app.repo_filter,
            &repo_rows(app),
            app.repo_selected,
        ),
        Screen::NewName => render_new_name(frame, app),
        Screen::Harness => render_picker(
            frame,
            "Harness",
            &app.harness_filter,
            &harness_rows(app),
            app.harness_selected,
        ),
    }
}

fn tree_rows(app: &App) -> Vec<(String, String)> {
    let mut rows = vec![("+ new tree".to_string(), String::new())];
    rows.extend(app.tree_matches.iter().map(|&i| {
        let row = &app.trees[i];
        (format!("{}/{}", row.repo, row.tree), row.status.clone())
    }));
    rows
}

fn repo_rows(app: &App) -> Vec<(String, String)> {
    app.repo_matches
        .iter()
        .map(|&i| (app.repos[i].clone(), String::new()))
        .collect()
}

fn harness_rows(app: &App) -> Vec<(String, String)> {
    app.harness_matches
        .iter()
        .map(|&i| (app.harnesses[i].clone(), String::new()))
        .collect()
}

fn layout(area: Rect) -> (Rect, Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
    (chunks[0], chunks[1], chunks[2])
}

fn render_picker(
    frame: &mut Frame,
    title: &str,
    filter: &str,
    rows: &[(String, String)],
    selected: usize,
) {
    let (header, body, footer) = layout(frame.area());
    render_filter(frame, header, title, filter);

    let items: Vec<ListItem> = rows
        .iter()
        .map(|(label, status)| {
            let mut spans = vec![Span::raw(label.clone())];
            if !status.is_empty() {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    status.clone(),
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(list, body, &mut state);

    render_help(frame, footer);
}

fn render_message(frame: &mut Frame, title: &str, message: &str) {
    let (header, body, footer) = layout(frame.area());
    frame.render_widget(Paragraph::new(title), header);
    frame.render_widget(Paragraph::new(message), body);
    render_help(frame, footer);
}

fn render_new_name(frame: &mut Frame, app: &App) {
    let (header, body, footer) = layout(frame.area());
    render_filter(frame, header, "New tree: name it", &app.new_tree_name);
    if let Some(error) = &app.new_tree_error {
        frame.render_widget(
            Paragraph::new(error.as_str()).style(Style::default().fg(Color::Red)),
            body,
        );
    }
    render_help(frame, footer);
}

fn render_filter(frame: &mut Frame, area: Rect, title: &str, text: &str) {
    let line = Line::from(vec![
        Span::styled(
            title.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::raw(text.to_string()),
        Span::raw("\u{2588}"),
    ]);
    let block = Block::default().borders(Borders::BOTTOM);
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_help(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(HELP).style(Style::default().add_modifier(Modifier::DIM)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::super::state::App;
    use super::super::{PickInput, TreeRow};
    use super::render;

    fn sample_app() -> App {
        App::new(PickInput {
            trees: vec![TreeRow {
                repo: "wrk".into(),
                tree: "feature".into(),
                status: "succeeded".into(),
            }],
            repos: vec!["wrk".into()],
            harnesses: vec!["claude".into(), "codex".into()],
            default_harness: Some("codex".into()),
        })
    }

    #[test]
    fn tree_screen_renders_rows_and_help() {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = sample_app();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let contents = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(contents.contains("new tree"));
        assert!(contents.contains("wrk/feature"));
        assert!(contents.contains("succeeded"));
        assert!(contents.contains("Enter select"));
    }

    #[test]
    fn typed_filter_text_is_visible() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = sample_app();
        for c in "feat".chars() {
            app.handle(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let first_line: String = terminal.backend().buffer().content[..60]
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(first_line.contains("Tree"));
        assert!(first_line.contains("feat"));
    }
}
