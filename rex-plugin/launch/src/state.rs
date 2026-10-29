use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use nucleo_matcher::{Config, Matcher};
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use wrk::tui::filter::{delete_word, fuzzy_filter, is_ctrl, validate_tree_name};

use crate::layout::{self, Anchor, Layout};

pub const AGENTS: [&str; 3] = ["claude", "codex", "devin"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub identifier: String,
    pub title: String,
    pub branch_name: String,
}

pub enum Tickets {
    Loading,
    Ready(Vec<Ticket>),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Menu {
    Closed,
    /// `choices` holds indexes into `App::repos` that match `filter`.
    Repo {
        filter: String,
        cursor: usize,
        choices: Vec<usize>,
    },
    Agent {
        cursor: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub name: String,
    pub branch: String,
    pub repo: String,
    pub agent: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Continue,
    Start(Request),
    Quit,
}

/// What a click landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    RepoChip,
    AgentChip,
    /// An item in the open dropdown, by position in its list.
    MenuItem(usize),
    /// The open dropdown's border or filter row.
    MenuFrame,
    /// A list row: 0 is the "new tree" row.
    Row(usize),
    Branch,
    Nothing,
}

/// Where the open dropdown sits and which of its items are visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuGeometry {
    pub rect: Rect,
    pub anchor: Anchor,
    pub has_filter: bool,
    pub first_item: usize,
    pub visible: usize,
}

pub struct App {
    pub query: String,
    pub branch: String,
    pub branch_edited: bool,
    pub editing_branch: bool,
    pub tickets: Tickets,
    /// Ticket index and match positions for each visible ticket row.
    pub matches: Vec<(usize, Vec<u32>)>,
    /// Row 0 is the "new tree" row; ticket rows follow.
    pub selected: usize,
    pub menu: Menu,
    pub repos: Vec<String>,
    pub repo: usize,
    pub agent: usize,
    pub error: Option<String>,
    haystacks: Vec<String>,
    matcher: Matcher,
}

impl App {
    pub fn new(repos: Vec<String>, repo: usize, agent: usize) -> Self {
        App {
            query: String::new(),
            branch: String::new(),
            branch_edited: false,
            editing_branch: false,
            tickets: Tickets::Loading,
            matches: Vec::new(),
            selected: 0,
            menu: Menu::Closed,
            repos,
            repo,
            agent,
            error: None,
            haystacks: Vec::new(),
            matcher: Matcher::new(Config::DEFAULT),
        }
    }

    /// A refresh keeps the selected ticket (or the "new" row) in place. A
    /// failed refresh keeps a list that is already showing.
    pub fn set_tickets(&mut self, result: Result<Vec<Ticket>, String>) {
        let showing = matches!(self.tickets, Tickets::Ready(_));
        let keep = self.selected_ticket().map(|t| t.identifier.clone());
        match result {
            Ok(tickets) => {
                let width = tickets
                    .iter()
                    .map(|t| t.identifier.len())
                    .max()
                    .unwrap_or(0);
                self.haystacks = tickets
                    .iter()
                    .map(|t| format!("{:<width$}  {}", t.identifier, t.title))
                    .collect();
                self.tickets = Tickets::Ready(tickets);
            }
            Err(_) if showing => return,
            Err(message) => self.tickets = Tickets::Failed(message),
        }
        self.matches = fuzzy_filter(&mut self.matcher, &self.query, &self.haystacks);
        if !showing {
            self.selected = self.exact_ticket_row().unwrap_or(0);
        } else {
            self.selected = keep.and_then(|id| self.row_of(&id)).unwrap_or(0);
        }
        self.sync_branch();
    }

    /// The typed text as a tree name: runs of whitespace become `-`.
    pub fn typed_name(&self) -> String {
        self.query.split_whitespace().collect::<Vec<_>>().join("-")
    }

    pub fn selected_ticket(&self) -> Option<&Ticket> {
        let Tickets::Ready(tickets) = &self.tickets else {
            return None;
        };
        let (index, _) = self.matches.get(self.selected.checked_sub(1)?)?;
        tickets.get(*index)
    }

    pub fn haystack(&self, ticket_index: usize) -> &str {
        &self.haystacks[ticket_index]
    }

    pub fn rows(&self) -> usize {
        1 + self.matches.len()
    }

    pub fn layout(&self, area: Rect) -> Option<Layout> {
        layout::compute(area, &self.repos[self.repo], AGENTS[self.agent])
    }

    /// The labels, cursor and filter text of the open dropdown.
    pub fn menu_items(&self) -> Option<(Vec<&str>, usize, Option<&str>)> {
        match &self.menu {
            Menu::Closed => None,
            Menu::Repo {
                filter,
                cursor,
                choices,
            } => Some((
                choices.iter().map(|&i| self.repos[i].as_str()).collect(),
                *cursor,
                Some(filter.as_str()),
            )),
            Menu::Agent { cursor } => Some((AGENTS.to_vec(), *cursor, None)),
        }
    }

    pub fn menu_geometry(&self, layout: &Layout) -> Option<MenuGeometry> {
        let (labels, cursor, filter) = self.menu_items()?;
        let anchor = match self.menu {
            Menu::Repo { .. } => Anchor::Repo,
            _ => Anchor::Agent,
        };
        let widest = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let filter_width = filter.map_or(0, |_| 12);
        let content_width = widest.max(filter_width).max(10) as u16;
        let has_filter = filter.is_some();
        let rows = u16::from(has_filter) + labels.len().max(1) as u16;
        let rect = layout::menu_rect(layout, anchor, content_width, rows);
        let visible = (rect.height.saturating_sub(2 + u16::from(has_filter))) as usize;
        Some(MenuGeometry {
            rect,
            anchor,
            has_filter,
            first_item: layout::window_start(cursor, visible),
            visible,
        })
    }

    pub fn hit(&self, area: Rect, col: u16, row: u16) -> Hit {
        let Some(layout) = self.layout(area) else {
            return Hit::Nothing;
        };
        if layout::contains(layout.repo_chip, col, row) {
            return Hit::RepoChip;
        }
        if layout::contains(layout.agent_chip, col, row) {
            return Hit::AgentChip;
        }
        if let Some(menu) = self.menu_geometry(&layout) {
            if !layout::contains(menu.rect, col, row) {
                return Hit::Nothing;
            }
            let top = menu.rect.y + 1 + u16::from(menu.has_filter);
            let (labels, _, _) = self.menu_items().unwrap_or_default();
            let inside = col > menu.rect.x && col + 1 < menu.rect.right();
            if inside && row >= top {
                let index = menu.first_item + (row - top) as usize;
                if ((row - top) as usize) < menu.visible && index < labels.len() {
                    return Hit::MenuItem(index);
                }
            }
            return Hit::MenuFrame;
        }
        if layout::contains(layout.list, col, row) {
            let index = layout::window_start(self.selected, layout.list.height as usize)
                + (row - layout.list.y) as usize;
            if index < self.rows() {
                return Hit::Row(index);
            }
        }
        if layout::contains(layout.branch, col, row) {
            return Hit::Branch;
        }
        Hit::Nothing
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, area: Rect) -> Step {
        self.error = None;
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = self.hit(area, event.column, event.row);
                self.click(hit)
            }
            MouseEventKind::ScrollUp => self.scroll(-1),
            MouseEventKind::ScrollDown => self.scroll(1),
            _ => Step::Continue,
        }
    }

    /// Clicking the selected row again starts it.
    fn click(&mut self, hit: Hit) -> Step {
        if self.menu != Menu::Closed {
            match hit {
                Hit::MenuItem(index) => self.pick(index),
                Hit::MenuFrame => {}
                Hit::RepoChip if matches!(self.menu, Menu::Repo { .. }) => self.menu = Menu::Closed,
                Hit::AgentChip if matches!(self.menu, Menu::Agent { .. }) => {
                    self.menu = Menu::Closed
                }
                Hit::RepoChip => self.open_repo_menu(),
                Hit::AgentChip => self.open_agent_menu(),
                _ => self.menu = Menu::Closed,
            }
            return Step::Continue;
        }
        match hit {
            Hit::RepoChip => self.open_repo_menu(),
            Hit::AgentChip => self.open_agent_menu(),
            Hit::Row(row) if row == self.selected => return self.start(),
            Hit::Row(row) => {
                self.editing_branch = false;
                self.selected = row;
                self.sync_branch();
            }
            Hit::Branch => self.editing_branch = true,
            _ => {}
        }
        Step::Continue
    }

    fn scroll(&mut self, delta: isize) -> Step {
        if self.menu == Menu::Closed {
            self.move_selection(delta);
        } else {
            self.move_menu(delta);
        }
        Step::Continue
    }

    fn open_repo_menu(&mut self) {
        self.menu = Menu::Repo {
            filter: String::new(),
            cursor: self.repo,
            choices: (0..self.repos.len()).collect(),
        };
    }

    fn open_agent_menu(&mut self) {
        self.menu = Menu::Agent { cursor: self.agent };
    }

    fn pick(&mut self, index: usize) {
        match &self.menu {
            Menu::Repo { choices, .. } => {
                if let Some(&repo) = choices.get(index) {
                    self.repo = repo;
                }
            }
            Menu::Agent { .. } => self.agent = index.min(AGENTS.len() - 1),
            Menu::Closed => {}
        }
        self.menu = Menu::Closed;
    }

    fn move_menu(&mut self, delta: isize) {
        let len = self.menu_items().map_or(0, |(labels, _, _)| labels.len());
        let (Menu::Repo { cursor, .. } | Menu::Agent { cursor }) = &mut self.menu else {
            return;
        };
        *cursor = cursor
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
    }

    fn edit_repo_filter(&mut self, change: impl FnOnce(&mut String)) {
        let Menu::Repo { filter, .. } = &mut self.menu else {
            return;
        };
        change(filter);
        let found = fuzzy_filter(&mut self.matcher, filter, &self.repos);
        if let Menu::Repo {
            cursor, choices, ..
        } = &mut self.menu
        {
            *choices = found.into_iter().map(|(i, _)| i).collect();
            *cursor = 0;
        }
    }

    fn row_of(&self, id: &str) -> Option<usize> {
        let Tickets::Ready(tickets) = &self.tickets else {
            return None;
        };
        let row = self
            .matches
            .iter()
            .position(|(i, _)| tickets[*i].identifier == id)?;
        Some(row + 1)
    }

    fn refilter(&mut self) {
        self.matches = fuzzy_filter(&mut self.matcher, &self.query, &self.haystacks);
        self.selected = self.exact_ticket_row().unwrap_or(0);
        self.sync_branch();
    }

    fn exact_ticket_row(&self) -> Option<usize> {
        let Tickets::Ready(tickets) = &self.tickets else {
            return None;
        };
        let typed = self.query.trim();
        let row = self
            .matches
            .iter()
            .position(|(i, _)| tickets[*i].identifier.eq_ignore_ascii_case(typed))?;
        Some(row + 1)
    }

    fn sync_branch(&mut self) {
        if self.branch_edited {
            return;
        }
        self.branch = match self.selected_ticket() {
            Some(ticket) => ticket
                .branch_name
                .split_once('/')
                .map_or(ticket.branch_name.as_str(), |(_, rest)| rest)
                .to_string(),
            None => self.typed_name(),
        };
    }

    pub fn handle(&mut self, key: KeyEvent) -> Step {
        self.error = None;
        if self.menu != Menu::Closed {
            return self.handle_menu(key);
        }
        if is_ctrl(key) {
            return self.handle_ctrl(key);
        }
        match key.code {
            KeyCode::Enter => return self.start(),
            KeyCode::Esc if self.editing_branch => self.editing_branch = false,
            KeyCode::Esc => return Step::Quit,
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Backspace => self.edit(|s| {
                s.pop();
            }),
            KeyCode::Char(c) => self.edit(|s| s.push(c)),
            _ => {}
        }
        Step::Continue
    }

    fn handle_ctrl(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Char('c') => return Step::Quit,
            KeyCode::Char('p') => self.open_repo_menu(),
            KeyCode::Char('o') => self.open_agent_menu(),
            KeyCode::Char('b') => self.editing_branch = !self.editing_branch,
            KeyCode::Char('w') => self.edit(delete_word),
            KeyCode::Char('u') => self.edit(String::clear),
            _ => {}
        }
        Step::Continue
    }

    fn handle_menu(&mut self, key: KeyEvent) -> Step {
        if is_ctrl(key) {
            match key.code {
                KeyCode::Char('c') => return Step::Quit,
                KeyCode::Char('p') => self.open_repo_menu(),
                KeyCode::Char('o') => self.open_agent_menu(),
                KeyCode::Char('u') => self.edit_repo_filter(String::clear),
                KeyCode::Char('w') => self.edit_repo_filter(delete_word),
                _ => {}
            }
            return Step::Continue;
        }
        match key.code {
            KeyCode::Up => self.move_menu(-1),
            KeyCode::Down => self.move_menu(1),
            KeyCode::Enter => {
                let cursor = self.menu_items().map_or(0, |(_, cursor, _)| cursor);
                self.pick(cursor);
            }
            KeyCode::Esc => self.menu = Menu::Closed,
            KeyCode::Backspace => self.edit_repo_filter(|s| {
                s.pop();
            }),
            KeyCode::Char(c) => self.edit_repo_filter(|s| s.push(c)),
            _ => {}
        }
        Step::Continue
    }

    fn move_selection(&mut self, delta: isize) {
        let last = self.rows() - 1;
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        self.sync_branch();
    }

    fn edit(&mut self, change: impl FnOnce(&mut String)) {
        if self.editing_branch {
            change(&mut self.branch);
            self.branch_edited = true;
        } else {
            change(&mut self.query);
            self.refilter();
        }
    }

    fn start(&mut self) -> Step {
        let name = match self.selected_ticket() {
            Some(ticket) => ticket.identifier.clone(),
            None => self.typed_name(),
        };
        if name.is_empty() {
            self.error = Some("type a name or pick a ticket".to_string());
            return Step::Continue;
        }
        if let Err(message) = validate_tree_name(&self.branch) {
            self.error = Some(message);
            return Step::Continue;
        }
        Step::Start(Request {
            name,
            branch: self.branch.clone(),
            repo: self.repos[self.repo].clone(),
            agent: AGENTS[self.agent],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn ticket(id: &str, title: &str, branch: &str) -> Ticket {
        Ticket {
            identifier: id.to_string(),
            title: title.to_string(),
            branch_name: branch.to_string(),
        }
    }

    fn app() -> App {
        let mut app = App::new(
            vec![
                "monorepo".to_string(),
                "jrepo".to_string(),
                "helm".to_string(),
            ],
            0,
            0,
        );
        app.set_tickets(Ok(vec![
            ticket("ML-616", "Rebase a job submit", "josh/ml-616-rebase-a-job"),
            ticket(
                "ML-617",
                "Agent builds one action",
                "josh/ml-617-agent-builds",
            ),
        ]));
        app
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            app.handle(key(KeyCode::Char(c)));
        }
    }

    fn start(app: &mut App) -> Request {
        match app.handle(key(KeyCode::Enter)) {
            Step::Start(request) => request,
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn enter_on_new_row_gives_exactly_the_typed_name_even_when_a_ticket_matches() {
        let mut app = app();
        type_text(&mut app, "ml-61");
        assert!(!app.matches.is_empty(), "a ticket should fuzzy-match");
        assert_eq!(app.selected, 0);
        let request = start(&mut app);
        assert_eq!(request.name, "ml-61");
        assert_eq!(request.branch, "ml-61");
    }

    #[test]
    fn typed_whitespace_becomes_dashes() {
        let mut app = app();
        type_text(&mut app, "ml-616 rebase");
        assert_eq!(start(&mut app).name, "ml-616-rebase");
    }

    #[test]
    fn exact_ticket_id_selects_that_ticket() {
        let mut app = app();
        type_text(&mut app, "ml-617");
        assert_eq!(app.selected_ticket().unwrap().identifier, "ML-617");
        let request = start(&mut app);
        assert_eq!(request.name, "ML-617");
        assert_eq!(request.branch, "ml-617-agent-builds");
    }

    #[test]
    fn arrow_picks_a_ticket_and_sets_the_branch_without_the_prefix() {
        let mut app = app();
        app.handle(key(KeyCode::Down));
        assert_eq!(app.selected_ticket().unwrap().identifier, "ML-616");
        assert_eq!(app.branch, "ml-616-rebase-a-job");
        app.handle(key(KeyCode::Up));
        assert_eq!(app.branch, "");
    }

    #[test]
    fn branch_edit_sticks_after_the_selection_changes() {
        let mut app = app();
        type_text(&mut app, "ml-61");
        app.handle(ctrl('b'));
        type_text(&mut app, "-x");
        app.handle(key(KeyCode::Esc));
        assert!(!app.editing_branch);
        app.handle(key(KeyCode::Down));
        assert!(app.selected_ticket().is_some());
        assert_eq!(app.branch, "ml-61-x");
        let request = start(&mut app);
        assert!(request.name.starts_with("ML-61"));
        assert_eq!(request.branch, "ml-61-x");
    }

    #[test]
    fn empty_name_does_not_start() {
        let mut app = app();
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert!(app.error.is_some());
    }

    #[test]
    fn invalid_branch_does_not_start() {
        let mut app = app();
        type_text(&mut app, "abc");
        app.handle(ctrl('b'));
        type_text(&mut app, "/x");
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert!(app.error.is_some());
    }

    #[test]
    fn repo_menu_picks_a_repo_and_esc_closes_it() {
        let mut app = app();
        app.handle(ctrl('p'));
        assert!(matches!(app.menu, Menu::Repo { cursor: 0, .. }));
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.menu, Menu::Closed);
        type_text(&mut app, "x");
        assert_eq!(start(&mut app).repo, "jrepo");

        app.handle(ctrl('p'));
        app.handle(key(KeyCode::Down));
        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Continue);
        assert_eq!(app.menu, Menu::Closed);
        assert_eq!(start(&mut app).repo, "jrepo");
    }

    #[test]
    fn typing_in_the_repo_menu_filters_it() {
        let mut app = app();
        app.handle(ctrl('p'));
        type_text(&mut app, "hel");
        let (labels, cursor, filter) = app.menu_items().unwrap();
        assert_eq!((labels, cursor, filter), (vec!["helm"], 0, Some("hel")));
        app.handle(key(KeyCode::Enter));
        type_text(&mut app, "x");
        assert_eq!(start(&mut app).repo, "helm");
        assert_eq!(app.query, "x");
    }

    #[test]
    fn repo_menu_with_no_match_picks_nothing() {
        let mut app = app();
        app.handle(ctrl('p'));
        type_text(&mut app, "zzzz");
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.menu, Menu::Closed);
        assert_eq!(app.repo, 0);
    }

    #[test]
    fn agent_menu_picks_an_agent() {
        let mut app = app();
        app.handle(ctrl('o'));
        type_text(&mut app, "zz");
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        type_text(&mut app, "x");
        assert_eq!(start(&mut app).agent, "codex");
    }

    fn click(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn wheel(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }
    }

    const AREA: Rect = Rect::new(0, 0, 80, 20);

    #[test]
    fn hit_finds_chips_rows_and_branch() {
        let app = app();
        let l = app.layout(AREA).unwrap();
        assert_eq!(app.hit(AREA, l.repo_chip.x, 0), Hit::RepoChip);
        assert_eq!(app.hit(AREA, l.agent_chip.x + 1, 0), Hit::AgentChip);
        assert_eq!(app.hit(AREA, 5, l.list.y), Hit::Row(0));
        assert_eq!(app.hit(AREA, 5, l.list.y + 2), Hit::Row(2));
        assert_eq!(app.hit(AREA, 5, l.list.y + 3), Hit::Nothing);
        assert_eq!(app.hit(AREA, 5, l.branch.y), Hit::Branch);
        assert_eq!(app.hit(AREA, 5, 19), Hit::Nothing);
    }

    #[test]
    fn hit_in_an_open_menu_finds_items_and_the_frame() {
        let mut app = app();
        app.handle(ctrl('p'));
        let l = app.layout(AREA).unwrap();
        let menu = app.menu_geometry(&l).unwrap();
        let x = menu.rect.x + 2;
        assert_eq!(app.hit(AREA, x, menu.rect.y), Hit::MenuFrame);
        assert_eq!(app.hit(AREA, x, menu.rect.y + 1), Hit::MenuFrame);
        assert_eq!(app.hit(AREA, x, menu.rect.y + 2), Hit::MenuItem(0));
        assert_eq!(app.hit(AREA, x, menu.rect.y + 4), Hit::MenuItem(2));
        assert_eq!(app.hit(AREA, x, menu.rect.y + 5), Hit::MenuFrame);
        assert_eq!(app.hit(AREA, 70, 12), Hit::Nothing);
    }

    #[test]
    fn clicking_a_chip_opens_its_menu_and_an_item_picks_it() {
        let mut app = app();
        let l = app.layout(AREA).unwrap();
        app.handle_mouse(click(l.repo_chip.x + 1, 0), AREA);
        assert!(matches!(app.menu, Menu::Repo { .. }));
        let menu = app.menu_geometry(&l).unwrap();
        app.handle_mouse(click(menu.rect.x + 2, menu.rect.y + 3), AREA);
        assert_eq!(app.menu, Menu::Closed);
        assert_eq!(app.repos[app.repo], "jrepo");
    }

    #[test]
    fn clicking_outside_an_open_menu_closes_it() {
        let mut app = app();
        app.handle(ctrl('o'));
        app.handle_mouse(click(3, 12), AREA);
        assert_eq!(app.menu, Menu::Closed);
        assert_eq!(app.agent, 0);
    }

    #[test]
    fn clicking_the_other_chip_switches_menus() {
        let mut app = app();
        let l = app.layout(AREA).unwrap();
        app.handle(ctrl('p'));
        app.handle_mouse(click(l.agent_chip.x + 1, 0), AREA);
        assert!(matches!(app.menu, Menu::Agent { .. }));
        app.handle_mouse(click(l.agent_chip.x + 1, 0), AREA);
        assert_eq!(app.menu, Menu::Closed);
    }

    #[test]
    fn clicking_a_row_selects_it_and_clicking_again_starts_it() {
        let mut app = app();
        let l = app.layout(AREA).unwrap();
        assert_eq!(
            app.handle_mouse(click(5, l.list.y + 1), AREA),
            Step::Continue
        );
        assert_eq!(app.selected, 1);
        assert_eq!(app.branch, "ml-616-rebase-a-job");
        let Step::Start(request) = app.handle_mouse(click(5, l.list.y + 1), AREA) else {
            panic!("second click should start");
        };
        assert_eq!(request.name, "ML-616");
    }

    #[test]
    fn clicking_the_branch_line_starts_editing_it() {
        let mut app = app();
        let l = app.layout(AREA).unwrap();
        app.handle_mouse(click(5, l.branch.y), AREA);
        assert!(app.editing_branch);
    }

    #[test]
    fn wheel_moves_through_the_list_and_through_menus() {
        let mut app = app();
        app.handle_mouse(wheel(MouseEventKind::ScrollDown), AREA);
        assert_eq!(app.selected, 1);
        app.handle_mouse(wheel(MouseEventKind::ScrollUp), AREA);
        assert_eq!(app.selected, 0);
        app.handle(ctrl('o'));
        app.handle_mouse(wheel(MouseEventKind::ScrollDown), AREA);
        assert_eq!(app.menu, Menu::Agent { cursor: 1 });
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn a_refresh_keeps_the_selected_ticket() {
        let mut app = app();
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Down));
        assert_eq!(app.selected_ticket().unwrap().identifier, "ML-617");
        app.set_tickets(Ok(vec![
            ticket("ML-600", "New one", "josh/ml-600-new"),
            ticket("ML-616", "Rebase a job submit", "josh/ml-616-rebase-a-job"),
            ticket(
                "ML-617",
                "Agent builds one action",
                "josh/ml-617-agent-builds",
            ),
        ]));
        assert_eq!(app.selected_ticket().unwrap().identifier, "ML-617");
        assert_eq!(app.branch, "ml-617-agent-builds");
    }

    #[test]
    fn a_refresh_keeps_the_new_row_selected() {
        let mut app = app();
        type_text(&mut app, "ml");
        assert_eq!(app.selected, 0);
        app.set_tickets(Ok(vec![ticket("ML-1", "First", "josh/ml-1-first")]));
        assert_eq!(app.selected, 0);
        assert_eq!(app.branch, "ml");
    }

    #[test]
    fn a_refresh_that_drops_the_ticket_falls_back_to_the_new_row() {
        let mut app = app();
        app.handle(key(KeyCode::Down));
        app.set_tickets(Ok(vec![ticket("ML-9", "Other", "josh/ml-9-other")]));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn a_failed_refresh_keeps_the_cached_list() {
        let mut app = app();
        app.set_tickets(Err("offline".to_string()));
        assert!(matches!(app.tickets, Tickets::Ready(_)));
        assert_eq!(app.matches.len(), 2);
    }

    #[test]
    fn esc_quits_when_no_menu_or_branch_edit_is_open() {
        let mut app = app();
        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Quit);
    }

    #[test]
    fn loading_and_failed_tickets_still_allow_plain_names() {
        let mut app = App::new(vec!["jrepo".to_string()], 0, 0);
        type_text(&mut app, "plain");
        assert_eq!(start(&mut app).name, "plain");
        app.set_tickets(Err("no key".to_string()));
        assert!(matches!(app.tickets, Tickets::Failed(_)));
        assert_eq!(start(&mut app).name, "plain");
    }
}
