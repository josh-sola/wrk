use crossterm::event::{KeyEvent, KeyModifiers};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};

pub(super) fn is_ctrl(key: KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
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

/// `^W` semantics: trailing whitespace goes first, then the word before it.
pub(super) fn delete_word(s: &mut String) {
    let trimmed = s.trim_end();
    let cut = trimmed
        .rfind(char::is_whitespace)
        .map(|i| i + 1)
        .unwrap_or(0);
    s.truncate(cut);
}

/// An empty query keeps every item in its original order.
pub(super) fn fuzzy_filter(
    matcher: &mut Matcher,
    query: &str,
    items: &[String],
) -> Vec<(usize, Vec<u32>)> {
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut scored: Vec<(usize, u32, Vec<u32>)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let mut buf = Vec::new();
            let mut indices = Vec::new();
            let haystack = Utf32Str::new(item, &mut buf);
            let score = pattern.indices(haystack, matcher, &mut indices)?;
            indices.sort_unstable();
            indices.dedup();
            Some((i, score, indices))
        })
        .collect();
    scored.sort_by_key(|&(_, score, _)| std::cmp::Reverse(score));
    scored.into_iter().map(|(i, _, idx)| (i, idx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_word_drops_trailing_word() {
        let mut s = "hello world".to_string();
        delete_word(&mut s);
        assert_eq!(s, "hello ");
        delete_word(&mut s);
        assert_eq!(s, "");
    }

    #[test]
    fn delete_word_drops_trailing_whitespace_first() {
        let mut s = "hello   ".to_string();
        delete_word(&mut s);
        assert_eq!(s, "");
    }

    #[test]
    fn delete_word_on_empty_string_is_a_no_op() {
        let mut s = String::new();
        delete_word(&mut s);
        assert_eq!(s, "");
    }
}
