use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Foreground colors for the picker. With `NO_COLOR` set, every color call
/// degrades to no foreground, leaving only bold/dim modifiers.
pub struct Palette {
    no_color: bool,
}

impl Palette {
    pub fn new(no_color: bool) -> Self {
        Palette { no_color }
    }

    fn color(&self, color: Color) -> Style {
        if self.no_color {
            Style::default()
        } else {
            Style::default().fg(color)
        }
    }

    pub fn accent(&self) -> Style {
        self.color(Color::Cyan)
    }

    pub fn accent_bold(&self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    pub fn green(&self) -> Style {
        self.color(Color::Green)
    }

    pub fn yellow(&self) -> Style {
        self.color(Color::Yellow)
    }

    pub fn red(&self) -> Style {
        self.color(Color::Red)
    }

    pub fn dim(&self) -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }
}

#[derive(Clone, Copy)]
pub enum StatusColor {
    Green,
    Yellow,
    Red,
    Dim,
}

impl StatusColor {
    pub fn style(self, palette: &Palette) -> Style {
        match self {
            StatusColor::Green => palette.green(),
            StatusColor::Yellow => palette.yellow(),
            StatusColor::Red => palette.red(),
            StatusColor::Dim => palette.dim(),
        }
    }
}

/// Maps a hook status string to the glyph and word the picker shows for it.
pub fn status_view(status: &str) -> (char, String, StatusColor) {
    match status {
        "succeeded" | "none" => ('✓', "ready".to_string(), StatusColor::Green),
        "pending" | "running" => ('●', "provisioning".to_string(), StatusColor::Yellow),
        "failed" => ('✗', "failed".to_string(), StatusColor::Red),
        "crashed" => ('✗', "crashed".to_string(), StatusColor::Red),
        other => ('?', other.to_string(), StatusColor::Dim),
    }
}

/// Shortens `text` to at most `width` chars, replacing the tail with `…`
/// when it doesn't fit.
pub fn truncate(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out: String = text.chars().take(width - 1).collect();
    out.push('…');
    out
}

/// Splits fuzzy-match indices computed over `"repo/tree"` back onto the
/// `repo` and `tree` cells they came from.
pub fn split_repo_tree_indices(indices: &[u32], repo_len: usize) -> (Vec<u32>, Vec<u32>) {
    let mut repo = Vec::new();
    let mut tree = Vec::new();
    for &i in indices {
        let pos = i as usize;
        if pos < repo_len {
            repo.push(i);
        } else if pos > repo_len {
            tree.push(i - repo_len as u32 - 1);
        }
    }
    (repo, tree)
}

/// Truncates and pads `text` to exactly `width` cells, highlighting the
/// chars at `indices` in the accent color. `bold` also bolds the rest.
pub fn cell_spans<'a>(
    text: &str,
    width: usize,
    indices: &[u32],
    palette: &Palette,
    bold: bool,
) -> Vec<Span<'a>> {
    let shown = truncate(text, width);
    let base = if bold {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut spans = highlighted_spans(&shown, indices, palette.accent_bold(), base);
    let pad = width.saturating_sub(shown.chars().count());
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    spans
}

/// Renders `text`, coloring the chars at `indices` with `matched` and the
/// rest with `base`. `indices` must be sorted.
pub fn highlighted_spans<'a>(
    text: &str,
    indices: &[u32],
    matched: Style,
    base: Style,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_matched = false;
    for (i, ch) in text.chars().enumerate() {
        let is_match = indices.binary_search(&(i as u32)).is_ok();
        if !current.is_empty() && is_match != current_matched {
            let style = if current_matched { matched } else { base };
            spans.push(Span::styled(std::mem::take(&mut current), style));
        }
        current.push(ch);
        current_matched = is_match;
    }
    if !current.is_empty() {
        let style = if current_matched { matched } else { base };
        spans.push(Span::styled(current, style));
    }
    spans
}

/// Joins `items` with " · ", dropping items from the end until the bottom
/// border's title -- `items` framed by `"─ "` and `" "`, inside a frame of
/// `width` columns -- fits with at least 2 cells free before hitting the
/// corner. This keeps the border from ever truncating an item mid-word.
pub fn fit_hint(items: &[&str], width: usize) -> String {
    let decoration = 2 + 3 + 2; // both corners, the "─ "/" " frame, 2 cells spare
    let budget = width.saturating_sub(decoration);
    let mut n = items.len();
    loop {
        let candidate = items[..n].join(" · ");
        if n == 0 || candidate.chars().count() <= budget {
            return candidate;
        }
        n -= 1;
    }
}

/// Builds the `harness  [claude]  codex  …` bottom bar, windowing the
/// harness list around the selection when it doesn't fit `width`.
pub fn harness_bar_line<'a>(
    harnesses: &[String],
    selected: usize,
    width: usize,
    palette: &Palette,
) -> Line<'a> {
    let label = "harness  ";
    if harnesses.is_empty() {
        return Line::from(Span::styled(label, palette.dim()));
    }
    let budget = width.saturating_sub(label.chars().count());
    let (start, end) = harness_window(harnesses, selected, budget);

    let mut spans = vec![Span::styled(label, palette.dim())];
    if start > 0 {
        spans.push(Span::styled("… ", palette.dim()));
    }
    for (offset, i) in (start..=end).enumerate() {
        if offset > 0 {
            spans.push(Span::raw("  "));
        }
        if i == selected {
            spans.push(Span::styled(
                format!("[{}]", harnesses[i]),
                palette.accent_bold(),
            ));
        } else {
            spans.push(Span::styled(harnesses[i].clone(), palette.dim()));
        }
    }
    if end + 1 < harnesses.len() {
        spans.push(Span::styled(" …", palette.dim()));
    }
    Line::from(spans)
}

fn harness_window(harnesses: &[String], selected: usize, budget: usize) -> (usize, usize) {
    let n = harnesses.len();
    let item_width = |i: usize| harnesses[i].chars().count() + if i == selected { 2 } else { 0 };

    let mut start = selected;
    let mut end = selected;
    let mut width = item_width(selected);
    loop {
        let can_left = start > 0;
        let can_right = end + 1 < n;
        if !can_left && !can_right {
            break;
        }
        let mut extended = false;
        if can_right {
            let reserve = if start > 0 { 2 } else { 0 };
            let candidate = width + 2 + item_width(end + 1);
            if candidate + reserve <= budget {
                end += 1;
                width = candidate;
                extended = true;
            }
        }
        if !extended && can_left {
            let reserve = if end + 1 < n { 2 } else { 0 };
            let candidate = width + 2 + item_width(start - 1);
            if candidate + reserve <= budget {
                start -= 1;
                width = candidate;
                extended = true;
            }
        }
        if !extended {
            break;
        }
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_text() {
        assert_eq!(truncate("feat", 10), "feat");
    }

    #[test]
    fn truncate_replaces_tail_with_ellipsis() {
        assert_eq!(truncate("feature-a", 5), "feat…");
        assert_eq!(truncate("feature-a", 1), "…");
        assert_eq!(truncate("feature-a", 0), "");
    }

    #[test]
    fn split_repo_tree_indices_drops_the_separator() {
        // "wrk/feature", matches on 'w','k','f' -> indices 0,2,4
        let (repo, tree) = split_repo_tree_indices(&[0, 2, 4], 3);
        assert_eq!(repo, vec![0, 2]);
        assert_eq!(tree, vec![0]);
    }

    #[test]
    fn highlighted_spans_splits_on_match_boundaries() {
        let matched = Style::default();
        let base = Style::default();
        let spans = highlighted_spans("feat", &[0, 1], matched, base);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "feat");
    }

    #[test]
    fn harness_window_keeps_selection_visible_when_too_narrow() {
        let harnesses: Vec<String> = ["claude", "codex", "devin", "pi"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (start, end) = harness_window(&harnesses, 3, 6);
        assert!(start <= 3 && 3 <= end);
    }

    #[test]
    fn fit_hint_keeps_every_item_when_it_fits() {
        let items = ["↵ open", "⇥ harness", "esc quit"];
        assert_eq!(fit_hint(&items, 80), "↵ open · ⇥ harness · esc quit");
    }

    #[test]
    fn fit_hint_drops_items_from_the_end_to_fit() {
        let items = ["↵ open", "⇥ harness", "esc quit", "^c quit"];
        let hint = fit_hint(&items, 20);
        assert!(items.iter().take_while(|i| hint.contains(**i)).count() >= 1);
        assert!(!hint.ends_with(' '));
        // Whatever is kept must be whole items, never a cut word.
        for part in hint.split(" · ") {
            assert!(items.contains(&part));
        }
    }

    #[test]
    fn fit_hint_on_zero_width_returns_empty() {
        let items = ["↵ open", "⇥ harness", "esc quit"];
        assert_eq!(fit_hint(&items, 0), "");
    }
}
