use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo_matcher::{Config, Matcher};

use super::filter::{clamp_selected, delete_word, fuzzy_filter, is_ctrl, validate_tree_name};
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
}

/// Fields are `pub(crate)` so `view` can read them without a wall of getters.
pub struct App {
    pub(crate) screen: Screen,
    matcher: Matcher,

    pub(crate) trees: Vec<TreeRow>,
    pub(crate) repos: Vec<String>,
    pub(crate) harnesses: Vec<String>,

    pub(crate) tree_filter: String,
    pub(crate) tree_matches: Vec<(usize, Vec<u32>)>,
    pub(crate) tree_selected: usize,

    pub(crate) repo_filter: String,
    pub(crate) repo_matches: Vec<(usize, Vec<u32>)>,
    pub(crate) repo_selected: usize,

    pub(crate) new_tree_name: String,
    pub(crate) new_tree_error: Option<String>,

    pub(crate) harness_selected: usize,

    pub(crate) target_repo: String,
}

impl App {
    pub fn new(input: PickInput) -> Self {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let tree_matches = fuzzy_filter(&mut matcher, "", &tree_labels(&input.trees));
        let repo_matches = fuzzy_filter(&mut matcher, "", &input.repos);
        let harness_selected = input
            .default_harness
            .as_ref()
            .and_then(|default| input.harnesses.iter().position(|h| h == default))
            .unwrap_or(0);
        App {
            screen: Screen::Tree,
            matcher,
            trees: input.trees,
            repos: input.repos,
            harnesses: input.harnesses,
            tree_filter: String::new(),
            tree_matches,
            tree_selected: 0,
            repo_filter: String::new(),
            repo_matches,
            repo_selected: 0,
            new_tree_name: String::new(),
            new_tree_error: None,
            harness_selected,
            target_repo: String::new(),
        }
    }

    pub fn handle(&mut self, key: KeyEvent) -> Step {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Step::Cancel;
        }
        match key.code {
            KeyCode::Tab => {
                self.cycle_harness(1);
                return Step::Continue;
            }
            KeyCode::BackTab => {
                self.cycle_harness(-1);
                return Step::Continue;
            }
            _ => {}
        }
        match self.screen {
            Screen::Tree => self.handle_tree(key),
            Screen::NewRepo => self.handle_new_repo(key),
            Screen::NewName => self.handle_new_name(key),
        }
    }

    fn cycle_harness(&mut self, delta: i32) {
        let len = self.harnesses.len();
        if len == 0 {
            return;
        }
        let current = self.harness_selected as i32;
        let next = (current + delta).rem_euclid(len as i32);
        self.harness_selected = next as usize;
    }

    fn current_harness(&self) -> String {
        self.harnesses
            .get(self.harness_selected)
            .cloned()
            .unwrap_or_default()
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
            KeyCode::Char('u') if is_ctrl(key) => {
                self.tree_filter.clear();
                self.refilter_trees();
            }
            KeyCode::Char('w') if is_ctrl(key) => {
                delete_word(&mut self.tree_filter);
                self.refilter_trees();
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
                } else if let Some(&(idx, _)) = self.tree_matches.get(self.tree_selected - 1) {
                    return Step::Done(GoTarget {
                        repo: self.trees[idx].repo.clone(),
                        tree: self.trees[idx].tree.clone(),
                        harness: self.current_harness(),
                        new: false,
                    });
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
                self.repo_selected = clamp_selected(self.repo_selected + 1, self.repo_matches.len())
            }
            KeyCode::Char('p') if is_ctrl(key) => {
                self.repo_selected = self.repo_selected.saturating_sub(1)
            }
            KeyCode::Char('n') if is_ctrl(key) => {
                self.repo_selected = clamp_selected(self.repo_selected + 1, self.repo_matches.len())
            }
            KeyCode::Char('u') if is_ctrl(key) => {
                self.repo_filter.clear();
                self.refilter_repos();
            }
            KeyCode::Char('w') if is_ctrl(key) => {
                delete_word(&mut self.repo_filter);
                self.refilter_repos();
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
                if let Some(&(idx, _)) = self.repo_matches.get(self.repo_selected) {
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
            KeyCode::Char('u') if is_ctrl(key) => {
                self.new_tree_name.clear();
                self.new_tree_error = None;
            }
            KeyCode::Char('w') if is_ctrl(key) => {
                delete_word(&mut self.new_tree_name);
                self.new_tree_error = None;
            }
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
                    return Step::Done(GoTarget {
                        repo: self.target_repo.clone(),
                        tree: self.new_tree_name.clone(),
                        harness: self.current_harness(),
                        new: true,
                    });
                }
                Err(message) => self.new_tree_error = Some(message),
            },
            _ => {}
        }
        Step::Continue
    }

    fn enter_new_repo(&mut self) {
        self.repo_filter.clear();
        self.refilter_repos();
        self.screen = Screen::NewRepo;
    }

    fn refilter_trees(&mut self) {
        let labels = tree_labels(&self.trees);
        self.tree_matches = fuzzy_filter(&mut self.matcher, &self.tree_filter, &labels);
        self.tree_selected = self.tree_selected.min(self.tree_matches.len());
    }

    fn refilter_repos(&mut self) {
        self.repo_matches = fuzzy_filter(&mut self.matcher, &self.repo_filter, &self.repos);
        self.repo_selected = clamp_selected(self.repo_selected, self.repo_matches.len());
    }
}

fn tree_labels(trees: &[TreeRow]) -> Vec<String> {
    trees
        .iter()
        .map(|t| format!("{}/{}", t.repo, t.tree))
        .collect()
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
    fn pick_existing_tree_returns_bar_harness() {
        let mut app = App::new(sample_input());
        assert_eq!(app.handle(key(KeyCode::Down)), Step::Continue);
        assert_eq!(app.screen, Screen::Tree);
        assert_eq!(app.tree_selected, 1);

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
        let app = App::new(sample_input());
        assert_eq!(app.harnesses[app.harness_selected], "codex");
    }

    #[test]
    fn no_default_harness_preselects_the_first() {
        let mut input = sample_input();
        input.default_harness = None;
        let app = App::new(input);
        assert_eq!(app.harness_selected, 0);
    }

    #[test]
    fn tab_and_backtab_cycle_and_wrap() {
        let mut app = App::new(sample_input());
        assert_eq!(app.harnesses[app.harness_selected], "codex");

        app.handle(key(KeyCode::Tab));
        assert_eq!(app.harnesses[app.harness_selected], "pi");
        app.handle(key(KeyCode::Tab));
        assert_eq!(app.harnesses[app.harness_selected], "claude");

        app.handle(key(KeyCode::BackTab));
        assert_eq!(app.harnesses[app.harness_selected], "pi");
        app.handle(key(KeyCode::BackTab));
        assert_eq!(app.harnesses[app.harness_selected], "codex");
    }

    #[test]
    fn filtering_narrows_and_reorders() {
        let mut app = App::new(sample_input());
        for c in "bugfix".chars() {
            app.handle(char_key(c));
        }
        // Only the bugfix row matches the full word.
        assert_eq!(app.tree_matches.len(), 1);
        assert_eq!(app.trees[app.tree_matches[0].0].tree, "bugfix");

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
            .map(|&(i, _)| app.trees[i].tree.as_str())
            .collect();
        assert_eq!(matched, vec!["foobar", "barfoo"]);
    }

    #[test]
    fn new_tree_full_flow_seeds_name_and_returns_harness() {
        let mut app = App::new(sample_input());
        for c in "hotfix".chars() {
            app.handle(char_key(c));
        }
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue); // + new tree
        assert_eq!(app.screen, Screen::NewRepo);

        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue); // pick first repo
        assert_eq!(app.screen, Screen::NewName);
        assert_eq!(app.new_tree_name, "hotfix");
        assert_eq!(app.target_repo, "wrk");

        app.handle(key(KeyCode::Tab)); // switch off the default harness
        let step = app.handle(key(KeyCode::Enter));
        assert_eq!(
            step,
            Step::Done(GoTarget {
                repo: "wrk".into(),
                tree: "hotfix".into(),
                harness: "pi".into(),
                new: true,
            })
        );
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
        let step = app.handle(key(KeyCode::Enter));
        assert!(matches!(step, Step::Done(_)));
    }

    #[test]
    fn ctrl_u_clears_the_active_field() {
        let mut app = App::new(sample_input());
        for c in "feat".chars() {
            app.handle(char_key(c));
        }
        app.handle(ctrl('u'));
        assert_eq!(app.tree_filter, "");

        app.enter_new_repo();
        for c in "wr".chars() {
            app.handle(char_key(c));
        }
        app.handle(ctrl('u'));
        assert_eq!(app.repo_filter, "");

        app.handle(key(KeyCode::Enter));
        for c in "name".chars() {
            app.handle(char_key(c));
        }
        app.handle(ctrl('u'));
        assert_eq!(app.new_tree_name, "");
    }

    #[test]
    fn ctrl_w_deletes_the_previous_word() {
        let mut app = App::new(sample_input());
        for c in "feat fix".chars() {
            app.handle(char_key(c));
        }
        app.handle(ctrl('w'));
        assert_eq!(app.tree_filter, "feat ");
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
