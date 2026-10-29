use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use wrk::tui::style::{Palette, highlighted_spans, truncate};

use crate::layout::{Layout, chip_text, window_start};
use crate::state::{AGENTS, App, Menu, Tickets};
use crate::theme::Theme;

pub fn render(frame: &mut Frame, app: &App, palette: &Palette, theme: &Theme) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(Block::new().style(Style::new().bg(theme.card)), area);
    let Some(lay) = app.layout(area) else {
        return;
    };

    let chip = palette.accent_bold().add_modifier(Modifier::REVERSED);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.edge).bg(theme.card))
        .padding(Padding::horizontal(1))
        .style(Style::new().bg(theme.card))
        .title_top(Line::from(Span::styled(chip_text(&app.repos[app.repo]), chip)).left_aligned())
        .title_top(Line::from(Span::styled(chip_text(AGENTS[app.agent]), chip)).right_aligned());
    frame.render_widget(block, lay.card);

    render_query(frame, app, palette, theme, lay.query);
    frame.render_widget(
        Paragraph::new(list_lines(app, palette, theme, lay.list))
            .style(Style::new().bg(theme.card)),
        lay.list,
    );
    render_branch(frame, app, palette, theme, lay.branch);
    render_footer(frame, app, palette, theme, lay.footer);
    render_menu(frame, app, palette, theme, &lay);
}

fn render_query(frame: &mut Frame, app: &App, palette: &Palette, theme: &Theme, area: Rect) {
    let line = if app.query.is_empty() {
        Line::from(Span::styled("ticket or name…", palette.dim()))
    } else {
        Line::from(truncate(&app.query, area.width as usize))
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::new().bg(theme.card)),
        area,
    );
    if !app.editing_branch && app.menu == Menu::Closed {
        let x = area.x + (app.query.chars().count() as u16).min(area.width.saturating_sub(1));
        frame.set_cursor_position(Position::new(x, area.y));
    }
}

fn render_branch(frame: &mut Frame, app: &App, palette: &Palette, theme: &Theme, area: Rect) {
    let style = if app.editing_branch {
        palette.accent_bold()
    } else if app.branch_edited {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        palette.dim()
    };
    let text = truncate(&app.branch, (area.width as usize).saturating_sub(2));
    let line = Line::from(vec![
        Span::styled("⎇ ", palette.dim()),
        Span::styled(text.clone(), style),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::new().bg(theme.card)),
        area,
    );
    if app.editing_branch && app.menu == Menu::Closed {
        let x = area.x + 2 + (text.chars().count() as u16).min(area.width.saturating_sub(3));
        frame.set_cursor_position(Position::new(x, area.y));
    }
}

fn render_footer(frame: &mut Frame, app: &App, palette: &Palette, theme: &Theme, area: Rect) {
    let line = if let Some(error) = &app.error {
        Line::from(Span::styled(format!(" {error}"), palette.red()))
    } else {
        let hints = if matches!(app.menu, Menu::Repo { .. }) {
            "⏎ choose  ↑↓ move  type to filter  esc back"
        } else if app.menu != Menu::Closed {
            "⏎ choose  ↑↓ move  esc back"
        } else if app.editing_branch {
            "⏎ start  ^B/esc done editing"
        } else {
            "⏎ start  ↑↓ pick  ^P repo  ^O agent  ^B branch  esc close"
        };
        Line::from(Span::styled(format!(" {hints}"), palette.dim()))
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::new().bg(theme.card)),
        area,
    );
}

fn picked(line: Line<'static>, is_picked: bool, theme: &Theme) -> Line<'static> {
    if is_picked {
        line.style(Style::new().bg(theme.picked))
    } else {
        line
    }
}

fn list_lines(app: &App, palette: &Palette, theme: &Theme, area: Rect) -> Vec<Line<'static>> {
    let width = area.width as usize;
    let mut rows: Vec<Line<'static>> = Vec::new();

    let name = app.typed_name();
    let new_text = if name.is_empty() {
        "new tree".to_string()
    } else {
        format!("new tree \"{name}\"")
    };
    rows.push(picked(
        Line::from(vec![
            Span::styled("+ ", palette.green()),
            Span::styled(
                truncate(&new_text, width.saturating_sub(2)),
                Style::new().add_modifier(Modifier::BOLD),
            ),
        ]),
        app.selected == 0,
        theme,
    ));

    for (row, (index, indices)) in app.matches.iter().enumerate() {
        let text = truncate(app.haystack(*index), width.saturating_sub(2));
        let mut spans = vec![Span::styled("◇ ", palette.yellow())];
        spans.extend(highlighted_spans(
            &text,
            indices,
            palette.accent_bold(),
            Style::new(),
        ));
        rows.push(picked(Line::from(spans), app.selected == row + 1, theme));
    }

    let note = match &app.tickets {
        Tickets::Loading => Some(Span::styled("  loading tickets…", palette.dim())),
        Tickets::Failed(message) => Some(Span::styled(
            format!("  {}", truncate(message, width.saturating_sub(2))),
            palette.red(),
        )),
        Tickets::Ready(_) if app.matches.is_empty() && !app.query.is_empty() => {
            Some(Span::styled("  no matching tickets", palette.dim()))
        }
        Tickets::Ready(_) => None,
    };
    if let Some(note) = note {
        rows.push(Line::from(note));
    }

    window(rows, app.selected, area.height as usize)
}

fn render_menu(frame: &mut Frame, app: &App, palette: &Palette, theme: &Theme, lay: &Layout) {
    let (Some(geo), Some((labels, cursor, filter))) = (app.menu_geometry(lay), app.menu_items())
    else {
        return;
    };
    let base = Style::new().bg(theme.card);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme.edge).bg(theme.card))
        .style(base);
    let inner = block.inner(geo.rect);
    frame.render_widget(Clear, geo.rect);
    frame.render_widget(block, geo.rect);

    let width = inner.width as usize;
    let mut rows: Vec<Line<'static>> = Vec::new();
    if let Some(filter) = filter {
        let line = if filter.is_empty() {
            Line::from(Span::styled(" filter…", palette.dim()))
        } else {
            Line::from(format!(" {}", truncate(filter, width.saturating_sub(1))))
        };
        rows.push(line);
        let x = inner.x + 1 + (filter.chars().count() as u16).min(inner.width.saturating_sub(2));
        frame.set_cursor_position(Position::new(x, inner.y));
    }
    if labels.is_empty() {
        rows.push(Line::from(Span::styled(" no match", palette.dim())));
    }
    let shown = labels
        .iter()
        .enumerate()
        .skip(geo.first_item)
        .take(geo.visible);
    for (i, label) in shown {
        let marker = if i == cursor { "› " } else { "  " };
        rows.push(picked(
            Line::from(vec![
                Span::styled(marker, palette.accent_bold()),
                Span::raw(truncate(label, width.saturating_sub(2))),
            ]),
            i == cursor,
            theme,
        ));
    }
    frame.render_widget(Paragraph::new(rows).style(base), inner);
}

/// Keeps `focus` inside the `height` rows shown.
fn window(rows: Vec<Line<'static>>, focus: usize, height: usize) -> Vec<Line<'static>> {
    let start = window_start(focus, height);
    rows.into_iter().skip(start).take(height).collect()
}
