use crossterm::event::{KeyEvent, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

pub(super) fn is_ctrl(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
}

pub(super) fn next_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1).min(len - 1)
    }
}

pub(super) fn clamp_selected(current: usize, len: usize) -> usize {
    if len == 0 { 0 } else { current.min(len - 1) }
}

pub(super) fn validate_tree_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("tree name is required".to_string());
    }
    if name.contains('/') {
        return Err("tree name may not contain /".to_string());
    }
    if name.chars().any(char::is_whitespace) {
        return Err("tree name may not contain whitespace".to_string());
    }
    Ok(())
}

/// Fuzzy-filters `items` on `query`, best match first. An empty query keeps
/// every item in its original order.
pub(super) fn fuzzy_filter(matcher: &mut Matcher, query: &str, items: &[String]) -> Vec<usize> {
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut scored: Vec<(usize, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let mut buf = Vec::new();
            pattern
                .score(Utf32Str::new(item, &mut buf), matcher)
                .map(|score| (i, score))
        })
        .collect();
    scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
    scored.into_iter().map(|(i, _)| i).collect()
}
