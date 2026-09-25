use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::{Config, Matcher};

use super::filter::{clamp_selected, fuzzy_filter, is_ctrl, next_index, validate_tree_name};
use super::{GoTarget, PickInput, TreeRow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Continue,
    Done(GoTarget),
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Tree,
    NewRepo,
    NewName,
    Harness,
}

/// Fields are `pub(crate)` so `view` can read them without a wall of getters.
pub struct App {
    pub(crate) screen: Screen,
    matcher: Matcher,

    pub(crate) trees: Vec<TreeRow>,
    pub(crate) repos: Vec<String>,
    pub(crate) harnesses: Vec<String>,
    default_harness: Option<String>,

    pub(crate) tree_filter: String,
    pub(crate) tree_matches: Vec<usize>,
    pub(crate) tree_selected: usize,

    pub(crate) repo_filter: String,
    pub(crate) repo_matches: Vec<usize>,
    pub(crate) repo_selected: usize,

    pub(crate) new_tree_name: String,
    pub(crate) new_tree_error: Option<String>,

    pub(crate) harness_filter: String,
    pub(crate) harness_matches: Vec<usize>,
    pub(crate) harness_selected: usize,

    target_repo: String,
    target_tree: String,
    target_new: bool,
    harness_came_from_new: bool,
}

impl App {
    pub fn new(input: PickInput) -> Self {
        let repo_matches: Vec<usize> = (0..input.repos.len()).collect();
        let tree_matches: Vec<usize> = (0..input.trees.len()).collect();
        App {
            screen: Screen::Tree,
            matcher: Matcher::new(Config::DEFAULT),
            trees: input.trees,
            repos: input.repos,
            harnesses: input.harnesses,
            default_harness: input.default_harness,
            tree_filter: String::new(),
            tree_matches,
            tree_selected: 0,
            repo_filter: String::new(),
            repo_matches,
            repo_selected: 0,
            new_tree_name: String::new(),
            new_tree_error: None,
            harness_filter: String::new(),
            harness_matches: Vec::new(),
            harness_selected: 0,
            target_repo: String::new(),
            target_tree: String::new(),
            target_new: false,
            harness_came_from_new: false,
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> Step {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Step::Cancel;
        }
        match self.screen {
            Screen::Tree => self.handle_tree(key),
            Screen::NewRepo => self.handle_new_repo(key),
            Screen::NewName => self.handle_new_name(key),
            Screen::Harness => self.handle_harness(key),
        }
    }

    fn handle_tree(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Esc => return Step::Cancel,
            KeyCode::Up => self.tree_selected = self.tree_selected.saturating_sub(1),
            KeyCode::Down => {
                self.tree_selected = (self.tree_selected + 1).min(self.tree_matches.len())
            }
            KeyCode::Char('p') if is_ctrl(key) => {
                self.tree_selected = self.tree_selected.saturating_sub(1)
            }
            KeyCode::Char('n') if is_ctrl(key) => {
                self.tree_selected = (self.tree_selected + 1).min(self.tree_matches.len())
            }
            KeyCode::Backspace => {
                self.tree_filter.pop();
                self.refilter_trees();
            }
            KeyCode::Char(c) if !is_ctrl(key) => {
                self.tree_filter.push(c);
                self.refilter_trees();
            }
            KeyCode::Enter => {
                if self.tree_selected == 0 {
                    self.enter_new_repo();
                } else if let Some(&idx) = self.tree_matches.get(self.tree_selected - 1) {
                    self.target_repo = self.trees[idx].repo.clone();
                    self.target_tree = self.trees[idx].tree.clone();
                    self.target_new = false;
                    self.harness_came_from_new = false;
                    self.enter_harness();
                }
            }
            _ => {}
        }
        Step::Continue
    }

    fn handle_new_repo(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Esc => self.screen = Screen::Tree,
            KeyCode::Up => self.repo_selected = self.repo_selected.saturating_sub(1),
            KeyCode::Down => {
                self.repo_selected = next_index(self.repo_selected, self.repo_matches.len())
            }
            KeyCode::Char('p') if is_ctrl(key) => {
                self.repo_selected = self.repo_selected.saturating_sub(1)
            }
            KeyCode::Char('n') if is_ctrl(key) => {
                self.repo_selected = next_index(self.repo_selected, self.repo_matches.len())
            }
            KeyCode::Backspace => {
                self.repo_filter.pop();
                self.refilter_repos();
            }
            KeyCode::Char(c) if !is_ctrl(key) => {
                self.repo_filter.push(c);
                self.refilter_repos();
            }
            KeyCode::Enter => {
                if let Some(&idx) = self.repo_matches.get(self.repo_selected) {
                    self.target_repo = self.repos[idx].clone();
                    self.new_tree_name = self.tree_filter.clone();
                    self.new_tree_error = None;
                    self.screen = Screen::NewName;
                }
            }
            _ => {}
        }
        Step::Continue
    }

    fn handle_new_name(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Esc => self.screen = Screen::NewRepo,
            KeyCode::Backspace => {
                self.new_tree_name.pop();
                self.new_tree_error = None;
            }
            KeyCode::Char(c) if !is_ctrl(key) => {
                self.new_tree_name.push(c);
                self.new_tree_error = None;
            }
            KeyCode::Enter => match validate_tree_name(&self.new_tree_name) {
                Ok(()) => {
                    self.target_tree = self.new_tree_name.clone();
                    self.target_new = true;
                    self.harness_came_from_new = true;
                    self.enter_harness();
                }
                Err(message) => self.new_tree_error = Some(message),
            },
            _ => {}
        }
        Step::Continue
    }

    fn handle_harness(&mut self, key: KeyEvent) -> Step {
        match key.code {
            KeyCode::Esc => {
                self.screen = if self.harness_came_from_new {
                    Screen::NewName
                } else {
                    Screen::Tree
                };
            }
            KeyCode::Up => self.harness_selected = self.harness_selected.saturating_sub(1),
            KeyCode::Down => {
                self.harness_selected =
                    next_index(self.harness_selected, self.harness_matches.len())
            }
            KeyCode::Char('p') if is_ctrl(key) => {
                self.harness_selected = self.harness_selected.saturating_sub(1)
            }
            KeyCode::Char('n') if is_ctrl(key) => {
                self.harness_selected =
                    next_index(self.harness_selected, self.harness_matches.len())
            }
            KeyCode::Backspace => {
                self.harness_filter.pop();
                self.refilter_harnesses();
            }
            KeyCode::Char(c) if !is_ctrl(key) => {
                self.harness_filter.push(c);
                self.refilter_harnesses();
            }
            KeyCode::Enter => {
                if let Some(&idx) = self.harness_matches.get(self.harness_selected) {
                    return Step::Done(GoTarget {
                        repo: self.target_repo.clone(),
                        tree: self.target_tree.clone(),
                        harness: self.harnesses[idx].clone(),
                        new: self.target_new,
                    });
                }
            }
            _ => {}
        }
        Step::Continue
    }

    fn enter_new_repo(&mut self) {
        self.repo_filter.clear();
        self.repo_selected = 0;
        self.refilter_repos();
        self.screen = Screen::NewRepo;
    }

    fn enter_harness(&mut self) {
        self.harness_filter.clear();
        self.refilter_harnesses();
        self.screen = Screen::Harness;
    }

    fn refilter_trees(&mut self) {
        let labels: Vec<String> = self
            .trees
            .iter()
            .map(|t| format!("{}/{}", t.repo, t.tree))
            .collect();
        self.tree_matches = fuzzy_filter(&mut self.matcher, &self.tree_filter, &labels);
        self.tree_selected = self.tree_selected.min(self.tree_matches.len());
    }

    fn refilter_repos(&mut self) {
        self.repo_matches = fuzzy_filter(&mut self.matcher, &self.repo_filter, &self.repos);
        self.repo_selected = clamp_selected(self.repo_selected, self.repo_matches.len());
    }

    fn refilter_harnesses(&mut self) {
        self.harness_matches =
            fuzzy_filter(&mut self.matcher, &self.harness_filter, &self.harnesses);
        self.harness_selected = if self.harness_filter.is_empty() {
            self.default_harness
                .as_ref()
                .and_then(|default| {
                    self.harness_matches
                        .iter()
                        .position(|&i| &self.harnesses[i] == default)
                })
                .unwrap_or(0)
        } else {
            0
        };
        self.harness_selected = clamp_selected(self.harness_selected, self.harness_matches.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn char_key(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn sample_input() -> PickInput {
        PickInput {
            trees: vec![
                TreeRow {
                    repo: "wrk".into(),
                    tree: "feature".into(),
                    status: "succeeded".into(),
                },
                TreeRow {
                    repo: "wrk".into(),
                    tree: "bugfix".into(),
                    status: "running".into(),
                },
                TreeRow {
                    repo: "other".into(),
                    tree: "main".into(),
                    status: "none".into(),
                },
                TreeRow {
                    repo: "zdemo".into(),
                    tree: "barfoo".into(),
                    status: "none".into(),
                },
                TreeRow {
                    repo: "demo".into(),
                    tree: "foobar".into(),
                    status: "none".into(),
                },
            ],
            repos: vec!["wrk".into(), "other".into()],
            harnesses: vec!["claude".into(), "codex".into(), "pi".into()],
            default_harness: Some("codex".into()),
        }
    }

    #[test]
    fn pick_existing_tree_then_harness() {
        let mut app = App::new(sample_input());
        assert_eq!(app.handle(key(KeyCode::Down)), Step::Continue);
        assert_eq!(app.screen, Screen::Tree);
        assert_eq!(app.tree_selected, 1);

        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert_eq!(app.screen, Screen::Harness);

        // Default harness is preselected with an empty filter.
        assert_eq!(
            app.harnesses[app.harness_matches[app.harness_selected]],
            "codex"
        );

        let step = app.handle(key(KeyCode::Enter));
        assert_eq!(
            step,
            Step::Done(GoTarget {
                repo: "wrk".into(),
                tree: "feature".into(),
                harness: "codex".into(),
                new: false,
            })
        );
    }

    #[test]
    fn default_harness_is_preselected() {
        let mut app = App::new(sample_input());
        app.target_repo = "wrk".into();
        app.target_tree = "feature".into();
        app.enter_harness();
        assert_eq!(
            app.harnesses[app.harness_matches[app.harness_selected]],
            "codex"
        );
    }

    #[test]
    fn filtering_narrows_and_reorders() {
        let mut app = App::new(sample_input());
        for c in "bugfix".chars() {
            app.handle(char_key(c));
        }
        // Only the bugfix row matches the full word.
        assert_eq!(app.tree_matches.len(), 1);
        assert_eq!(app.trees[app.tree_matches[0]].tree, "bugfix");

        for _ in 0.."bugfix".len() {
            app.handle(key(KeyCode::Backspace));
        }
        assert!(app.tree_filter.is_empty());

        for c in "foo".chars() {
            app.handle(char_key(c));
        }
        // Best match first: a prefix match outranks one buried mid-word,
        // regardless of each row's position in the underlying list.
        let matched: Vec<&str> = app
            .tree_matches
            .iter()
            .map(|&i| app.trees[i].tree.as_str())
            .collect();
        assert_eq!(matched, vec!["foobar", "barfoo"]);
    }

    #[test]
    fn new_tree_path_seeds_filter_text() {
        let mut app = App::new(sample_input());
        for c in "hotfix".chars() {
            app.handle(char_key(c));
        }
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue); // + new tree (row 0 stays selected)
        assert_eq!(app.screen, Screen::NewRepo);

        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue); // pick first repo
        assert_eq!(app.screen, Screen::NewName);
        assert_eq!(app.new_tree_name, "hotfix");
    }

    #[test]
    fn invalid_tree_name_is_rejected() {
        let mut app = App::new(sample_input());
        app.enter_new_repo();
        app.handle(key(KeyCode::Enter)); // pick first repo -> NewName
        assert_eq!(app.screen, Screen::NewName);

        for c in "bad name".chars() {
            app.handle(char_key(c));
        }
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert_eq!(app.screen, Screen::NewName);
        assert!(app.new_tree_error.is_some());

        for _ in 0.."bad name".len() {
            app.handle(key(KeyCode::Backspace));
        }
        for c in "goodname".chars() {
            app.handle(char_key(c));
        }
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert_eq!(app.screen, Screen::Harness);
    }

    #[test]
    fn esc_goes_back_at_each_step() {
        let mut app = App::new(sample_input());
        app.enter_new_repo();
        assert_eq!(app.screen, Screen::NewRepo);
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::NewName);

        app.handle(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::NewRepo);

        app.handle(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Tree);

        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Cancel);
    }

    #[test]
    fn esc_from_harness_returns_to_the_right_screen() {
        let mut app = App::new(sample_input());
        // Existing-tree path returns to Tree.
        app.handle(key(KeyCode::Down));
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Harness);
        app.handle(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::Tree);

        // New-tree path returns to NewName.
        app.enter_new_repo();
        app.handle(key(KeyCode::Enter));
        for c in "feat".chars() {
            app.handle(char_key(c));
        }
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::Harness);
        app.handle(key(KeyCode::Esc));
        assert_eq!(app.screen, Screen::NewName);
    }

    #[test]
    fn ctrl_c_cancels_from_anywhere() {
        let mut app = App::new(sample_input());
        app.enter_new_repo();
        app.handle(key(KeyCode::Enter));
        assert_eq!(app.screen, Screen::NewName);
        assert_eq!(app.handle(ctrl('c')), Step::Cancel);
    }

    #[test]
    fn no_repos_leaves_new_repo_screen_empty() {
        let mut input = sample_input();
        input.repos.clear();
        let mut app = App::new(input);
        app.enter_new_repo();
        assert!(app.repo_matches.is_empty());
        // Enter is a no-op with nothing to select.
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert_eq!(app.screen, Screen::NewRepo);
    }

    #[test]
    fn ctrl_n_and_ctrl_p_move_like_arrows() {
        let mut app = App::new(sample_input());
        app.handle(ctrl('n'));
        assert_eq!(app.tree_selected, 1);
        app.handle(ctrl('p'));
        assert_eq!(app.tree_selected, 0);
    }
}
