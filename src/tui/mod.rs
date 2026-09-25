mod filter;
mod state;
mod view;

use crossterm::event::{Event, KeyEventKind};

use crate::output::WrkError;
use state::{App, Step};

pub struct TreeRow {
    pub repo: String,
    pub tree: String,
    pub status: String,
}

pub struct PickInput {
    pub trees: Vec<TreeRow>,
    pub repos: Vec<String>,
    pub harnesses: Vec<String>,
    pub default_harness: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoTarget {
    pub repo: String,
    pub tree: String,
    pub harness: String,
    pub new: bool,
}

pub fn pick(input: PickInput) -> Result<Option<GoTarget>, WrkError> {
    let mut terminal = TerminalGuard::new()?;
    let mut app = App::new(input);

    loop {
        terminal.draw(|frame| view::render(frame, &app))?;

        let Event::Key(key) = crossterm::event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.handle(key) {
            Step::Continue => {}
            Step::Done(target) => return Ok(Some(target)),
            Step::Cancel => return Ok(None),
        }
    }
}

/// Owns the alternate screen and raw mode for the picker's lifetime. `Drop`
/// restores the terminal on every exit path, including an early return from
/// an `Err` and an unwind through `pick`.
struct TerminalGuard {
    terminal: ratatui::DefaultTerminal,
}

impl TerminalGuard {
    fn new() -> std::io::Result<Self> {
        // `try_init` also installs a panic hook that restores the terminal
        // before the default hook runs, covering the case where the guard's
        // `Drop` never gets a chance to.
        Ok(Self {
            terminal: ratatui::try_init()?,
        })
    }
}

impl std::ops::Deref for TerminalGuard {
    type Target = ratatui::DefaultTerminal;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl std::ops::DerefMut for TerminalGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
