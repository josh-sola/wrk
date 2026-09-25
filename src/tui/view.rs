use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph};

use super::TreeRow;
use super::state::{App, Screen};
use super::style::{
    Palette, cell_spans, fit_hint, harness_bar_line, split_repo_tree_indices, status_view,
};

/// Below this row-content width, the status column drops its word and
/// keeps only the colored glyph.
const MIN_CONTENT_WIDTH_FOR_STATUS_WORDS: usize = 44;

const MAX_PANEL_WIDTH: u16 = 80;
const MAX_PANEL_HEIGHT: u16 = 22;

/// Small panes, such as popups, keep every cell; larger screens get a margin
/// and a centered panel capped at a readable size.
fn panel_area(area: Rect) -> Rect {
    let margin_x = if area.width >= 60 { 2 } else { 0 };
    let margin_y = if area.height >= 16 { 1 } else { 0 };
    let width = (area.width - 2 * margin_x).min(MAX_PANEL_WIDTH);
    let height = (area.height - 2 * margin_y).min(MAX_PANEL_HEIGHT);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

pub fn render(frame: &mut Frame, app: &App, palette: &Palette) {
    let area = panel_area(frame.area());
    if area.width < 24 || area.height < 6 {
        render_centered(frame, area, "window too small", Style::default());
        return;
    }

    let step_title = match app.screen {
        Screen::Tree => "pick tree",
        Screen::NewRepo => "new tree › repo",
        Screen::NewName => "new tree › name",
    };
    let hint_items: &[&str] = match app.screen {
        Screen::Tree => &["↵ open", "⇥ harness", "esc quit"],
        Screen::NewRepo => &["↵ choose", "⇥ harness", "esc back"],
        Screen::NewName => &["↵ create", "⇥ harness", "esc back"],
    };
    let hint = fit_hint(hint_items, area.width as usize);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .title_top(Line::from("─ wrk go ").left_aligned())
        .title_top(Line::styled(format!(" {step_title} ─"), palette.dim()).right_aligned())
        .title_bottom(Line::styled(format!("─ {hint} "), palette.dim()).left_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);
    let (body, harness_row) = (sections[0], sections[1]);

    match app.screen {
        Screen::Tree => render_tree(frame, body, app, palette),
        Screen::NewRepo => render_new_repo(frame, body, app, palette),
        Screen::NewName => render_new_name(frame, body, app, palette),
    }

    let line = harness_bar_line(
        &app.harnesses,
        app.harness_selected,
        harness_row.width as usize,
        palette,
    );
    frame.render_widget(Paragraph::new(line), harness_row);
}

fn render_tree(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);
    render_query_header(
        frame,
        chunks[0],
        &app.tree_filter,
        app.tree_matches.len(),
        app.trees.len(),
        palette,
    );
    render_rule(frame, chunks[1]);
    render_tree_list(frame, chunks[2], app, palette);
}

fn render_tree_list(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let content_width = area.width as usize;
    let show_status_word = content_width >= MIN_CONTENT_WIDTH_FOR_STATUS_WORDS;
    let (repo_w, tree_w) = tree_column_widths(app, content_width, show_status_word);

    let mut items = Vec::with_capacity(app.tree_matches.len() + 1);
    items.push(new_tree_item(
        &app.tree_filter,
        app.tree_selected == 0,
        palette,
    ));
    for (row, &(idx, ref indices)) in app.tree_matches.iter().enumerate() {
        let selected = app.tree_selected == row + 1;
        items.push(tree_item(
            &app.trees[idx],
            indices,
            repo_w,
            tree_w,
            show_status_word,
            selected,
            palette,
        ));
    }

    let list = List::new(items).highlight_style(Style::default());
    let mut state = ListState::default();
    state.select(Some(app.tree_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn tree_column_widths(app: &App, content_width: usize, show_status_word: bool) -> (usize, usize) {
    let gutter = 2;
    let gaps = 6; // three spaces after the repo column, three after the tree column
    let status_reserve = if show_status_word { 14 } else { 1 };
    let budget = content_width
        .saturating_sub(gutter)
        .saturating_sub(gaps)
        .saturating_sub(status_reserve);

    let longest = |f: fn(&TreeRow) -> usize| {
        app.tree_matches
            .iter()
            .map(|&(i, _)| f(&app.trees[i]))
            .max()
            .unwrap_or(0)
    };
    let repo_w = longest(|t| t.repo.chars().count()).min(budget);
    let tree_w = longest(|t| t.tree.chars().count()).min(budget.saturating_sub(repo_w));
    (repo_w, tree_w)
}

fn new_tree_item(query: &str, selected: bool, palette: &Palette) -> ListItem<'static> {
    let text = if query.is_empty() {
        "+ new tree".to_string()
    } else {
        format!("+ new tree \"{query}\"")
    };
    let spans = vec![
        gutter_span(selected, palette),
        Span::styled(text, bold_if(Style::default(), selected)),
    ];
    ListItem::new(Line::from(spans))
}

fn tree_item(
    row: &TreeRow,
    indices: &[u32],
    repo_w: usize,
    tree_w: usize,
    show_status_word: bool,
    selected: bool,
    palette: &Palette,
) -> ListItem<'static> {
    let (repo_idx, tree_idx) = split_repo_tree_indices(indices, row.repo.chars().count());

    let mut spans = vec![gutter_span(selected, palette)];
    spans.extend(cell_spans(&row.repo, repo_w, &repo_idx, palette, selected));
    spans.push(Span::raw("   "));
    spans.extend(cell_spans(&row.tree, tree_w, &tree_idx, palette, selected));
    spans.push(Span::raw("   "));

    let (glyph, label, color) = status_view(&row.status);
    let status_style = bold_if(color.style(palette), selected);
    let status_text = if show_status_word {
        format!("{glyph} {label}")
    } else {
        glyph.to_string()
    };
    spans.push(Span::styled(status_text, status_style));

    ListItem::new(Line::from(spans))
}

fn render_new_repo(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    if app.repos.is_empty() {
        render_centered(
            frame,
            area,
            "no repos yet — run wrk clone <url>",
            palette.dim(),
        );
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);
    render_query_header(
        frame,
        chunks[0],
        &app.repo_filter,
        app.repo_matches.len(),
        app.repos.len(),
        palette,
    );
    render_rule(frame, chunks[1]);

    let width = chunks[2].width as usize;
    let items: Vec<ListItem> = app
        .repo_matches
        .iter()
        .enumerate()
        .map(|(row, &(idx, ref indices))| {
            let selected = app.repo_selected == row;
            let mut spans = vec![gutter_span(selected, palette)];
            spans.extend(cell_spans(
                &app.repos[idx],
                width.saturating_sub(2),
                indices,
                palette,
                selected,
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();
    let list = List::new(items).highlight_style(Style::default());
    let mut state = ListState::default();
    if !app.repo_matches.is_empty() {
        state.select(Some(app.repo_selected));
    }
    frame.render_stateful_widget(list, chunks[2], &mut state);
}

fn render_new_name(frame: &mut Frame, area: Rect, app: &App, palette: &Palette) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(format!("repo  {}", app.target_repo)).style(palette.dim()),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(format!("❯ {}█", app.new_tree_name)),
        chunks[1],
    );
    if let Some(error) = &app.new_tree_error {
        frame.render_widget(
            Paragraph::new(error.as_str()).style(palette.red()),
            chunks[2],
        );
    }
}

fn render_query_header(
    frame: &mut Frame,
    area: Rect,
    query: &str,
    matched: usize,
    total: usize,
    palette: &Palette,
) {
    let left = format!("❯ {query}█");
    let right = format!("{matched}/{total}");
    let width = area.width as usize;
    let pad = width
        .saturating_sub(left.chars().count())
        .saturating_sub(right.chars().count());
    let spans = vec![
        Span::raw(left),
        Span::raw(" ".repeat(pad)),
        Span::styled(right, palette.dim()),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_rule(frame: &mut Frame, area: Rect) {
    frame.render_widget(Paragraph::new("─".repeat(area.width as usize)), area);
}

fn render_centered(frame: &mut Frame, area: Rect, text: &str, style: Style) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(style),
        rows[1],
    );
}

fn gutter_span(selected: bool, palette: &Palette) -> Span<'static> {
    if selected {
        Span::styled("▌ ", palette.accent())
    } else {
        Span::raw("  ")
    }
}

fn bold_if(style: Style, bold: bool) -> Style {
    if bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use super::super::{PickInput, TreeRow};
    use super::*;

    fn sample_input() -> PickInput {
        PickInput {
            trees: vec![
                TreeRow {
                    repo: "myproj".into(),
                    tree: "feature-a".into(),
                    status: "succeeded".into(),
                },
                TreeRow {
                    repo: "myproj".into(),
                    tree: "feature-b".into(),
                    status: "pending".into(),
                },
                TreeRow {
                    repo: "api".into(),
                    tree: "feat-login".into(),
                    status: "failed".into(),
                },
            ],
            repos: vec!["myproj".into(), "api".into()],
            harnesses: vec!["claude".into(), "codex".into(), "devin".into(), "pi".into()],
            default_harness: Some("claude".into()),
        }
    }

    fn render_lines(app: &App, palette: &Palette, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app, palette)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| {
                        buffer
                            .cell((x, y))
                            .map(ratatui::buffer::Cell::symbol)
                            .unwrap_or(" ")
                    })
                    .collect::<String>()
            })
            .collect()
    }

    fn assert_no_background(app: &App, palette: &Palette, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, app, palette)).unwrap();
        for cell in terminal.backend().buffer().content() {
            assert_eq!(cell.bg, Color::Reset, "found a painted background cell");
        }
    }

    #[test]
    fn tree_screen_80x20_shows_frame_rows_and_harness_bar() {
        let mut app = App::new(sample_input());
        for c in "feat".chars() {
            app.handle(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 80, 20);
        let all = lines.join("\n");

        assert!(lines[0].trim().is_empty());
        assert!(lines[1].starts_with("  ╭─ wrk go "));
        assert!(lines[1].contains("pick tree"));
        assert!(lines[1].ends_with("╮  "));
        assert!(all.contains("+ new tree \"feat\""));
        assert!(all.contains("3/3") || all.contains("feat"));
        assert!(all.contains("✓ ready"));
        assert!(all.contains("● provisioning"));
        assert!(all.contains("✗ failed"));
        assert!(all.contains("[claude]"));
        assert!(lines[lines.len() - 2].starts_with("  ╰"));
        assert!(lines[lines.len() - 2].ends_with("╯  "));
        assert_no_background(&app, &palette, 80, 20);
    }

    #[test]
    fn large_screen_centers_a_capped_panel() {
        let app = App::new(sample_input());
        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 120, 40);

        let top = lines.iter().position(|l| l.contains('╭')).unwrap();
        let bottom = lines.iter().position(|l| l.contains('╰')).unwrap();
        assert_eq!(bottom - top + 1, usize::from(MAX_PANEL_HEIGHT));
        assert_eq!(top, (40 - usize::from(MAX_PANEL_HEIGHT)) / 2);

        let left = lines[top].chars().position(|c| c == '╭').unwrap();
        let right = lines[top].chars().position(|c| c == '╮').unwrap();
        assert_eq!(right - left + 1, usize::from(MAX_PANEL_WIDTH));
        assert_eq!(left, (120 - usize::from(MAX_PANEL_WIDTH)) / 2);
    }

    #[test]
    fn small_pane_uses_every_cell() {
        let app = App::new(sample_input());
        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 40, 10);

        assert!(lines[0].starts_with('╭'));
        assert!(lines[9].ends_with('╯'));
    }

    #[test]
    fn tree_screen_40x10_hides_status_words_but_keeps_marks() {
        let app = App::new(sample_input());
        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 40, 10);
        let all = lines.join("\n");

        assert!(!all.contains("ready"));
        assert!(!all.contains("provisioning"));
        assert!(all.contains('✓'));
        assert_no_background(&app, &palette, 40, 10);
    }

    #[test]
    fn tiny_20x5_shows_window_too_small() {
        let app = App::new(sample_input());
        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 20, 5);
        assert!(lines.iter().any(|l| l.contains("window too small")));
    }

    #[test]
    fn does_not_panic_at_1x1() {
        let app = App::new(sample_input());
        let palette = Palette::new(false);
        let _ = render_lines(&app, &palette, 1, 1);
    }

    #[test]
    fn name_screen_80x14_shows_context_and_error() {
        let mut app = App::new(sample_input());
        app.handle(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        app.handle(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        for c in "bad name".chars() {
            app.handle(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        app.handle(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 80, 14);
        let all = lines.join("\n");
        assert!(all.contains("repo  myproj"));
        assert!(all.contains("bad name"));
        assert!(all.contains("may not contain whitespace"));
        assert!(all.contains("new tree › name"));
    }

    #[test]
    fn tree_screen_40x10_bottom_border_has_no_truncated_hint() {
        let app = App::new(sample_input());
        let palette = Palette::new(false);
        let lines = render_lines(&app, &palette, 40, 10);
        let bottom = lines.last().unwrap();

        assert!(bottom.contains("↵ open · ⇥ harness · esc quit"));
        assert!(bottom.starts_with('╰'));
        assert!(bottom.ends_with('╯'));
    }
}
