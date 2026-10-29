use ratatui::layout::Rect;

/// Where each part of the composer sits for a given terminal size. The view
/// draws from these rects and mouse hit-testing reads the same ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub card: Rect,
    pub footer: Rect,
    pub repo_chip: Rect,
    pub agent_chip: Rect,
    pub query: Rect,
    pub list: Rect,
    pub branch: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Repo,
    Agent,
}

pub fn chip_text(label: &str) -> String {
    format!(" {label} ▾ ")
}

fn width_of(text: &str) -> u16 {
    text.chars().count() as u16
}

/// None when the terminal is too small to draw the card.
pub fn compute(area: Rect, repo_label: &str, agent_label: &str) -> Option<Layout> {
    if area.width < 24 || area.height < 6 {
        return None;
    }
    let card = Rect::new(area.x, area.y, area.width, area.height - 1);
    let footer = Rect::new(area.x, card.bottom(), area.width, 1);
    let repo_width = width_of(&chip_text(repo_label));
    let agent_width = width_of(&chip_text(agent_label));
    let repo_chip = Rect::new(card.x + 1, card.y, repo_width, 1);
    let agent_x = (card.right() - 1).saturating_sub(agent_width);
    let agent_chip = Rect::new(agent_x, card.y, agent_width, 1);

    let inner = Rect::new(card.x + 2, card.y + 1, card.width - 4, card.height - 2);
    if inner.height < 3 {
        return None;
    }
    Some(Layout {
        card,
        footer,
        repo_chip,
        agent_chip,
        query: Rect::new(inner.x, inner.y, inner.width, 1),
        list: Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 2),
        branch: Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
    })
}

/// The box for a dropdown under its chip. `content_width` and `rows` size the
/// text inside the border.
pub fn menu_rect(layout: &Layout, anchor: Anchor, content_width: u16, rows: u16) -> Rect {
    let chip = match anchor {
        Anchor::Repo => layout.repo_chip,
        Anchor::Agent => layout.agent_chip,
    };
    let width = (content_width + 4).max(chip.width).min(layout.card.width);
    let x = match anchor {
        Anchor::Repo => chip.x,
        Anchor::Agent => chip.right().saturating_sub(width).max(layout.card.x),
    };
    let x = x.min(layout.card.right().saturating_sub(width));
    let room = layout.footer.y.saturating_sub(chip.y + 1);
    let height = (rows + 2).min(room);
    Rect::new(x, chip.y + 1, width, height)
}

/// First row shown when `focus` has to stay inside `height` rows.
pub fn window_start(focus: usize, height: usize) -> usize {
    (focus + 1).saturating_sub(height)
}

pub fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.right() && row >= rect.y && row < rect.bottom()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        compute(Rect::new(0, 0, 80, 20), "monorepo", "claude").unwrap()
    }

    #[test]
    fn too_small_has_no_layout() {
        assert!(compute(Rect::new(0, 0, 0, 0), "a", "b").is_none());
        assert!(compute(Rect::new(0, 0, 20, 20), "a", "b").is_none());
    }

    #[test]
    fn chips_sit_on_the_top_border_at_each_end() {
        let l = layout();
        assert_eq!(l.repo_chip, Rect::new(1, 0, 12, 1));
        assert_eq!(l.agent_chip.right(), 79);
        assert_eq!(l.footer.y, 19);
        assert_eq!(l.branch.y, 17);
    }

    #[test]
    fn menu_hangs_under_its_chip() {
        let l = layout();
        let repo = menu_rect(&l, Anchor::Repo, 8, 4);
        assert_eq!((repo.x, repo.y, repo.height), (l.repo_chip.x, 1, 6));
        let agent = menu_rect(&l, Anchor::Agent, 6, 3);
        assert_eq!(agent.right(), l.agent_chip.right());
        assert_eq!(agent.y, 1);
    }

    #[test]
    fn menu_height_stops_above_the_footer() {
        let l = layout();
        assert_eq!(menu_rect(&l, Anchor::Repo, 8, 50).bottom(), l.footer.y);
    }

    #[test]
    fn window_keeps_the_focus_visible() {
        assert_eq!(window_start(0, 5), 0);
        assert_eq!(window_start(4, 5), 0);
        assert_eq!(window_start(7, 5), 3);
    }
}
