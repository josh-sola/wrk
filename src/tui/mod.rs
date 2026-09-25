mod filter;
mod state;
mod style;
mod view;

use std::fs::{File, OpenOptions};

use crossterm::event::{Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::output::WrkError;
use state::{App, Step};
use style::Palette;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    Centered,
    /// Uses every cell, for hosts such as popups that already pad the picker.
    Fill,
}

/// Draws on `/dev/tty`, never stdout, so `wrk pick --json` output stays clean.
pub fn pick(input: PickInput, sizing: Sizing) -> Result<Option<GoTarget>, WrkError> {
    let mut terminal = TerminalGuard::new()?;
    let mut app = App::new(input);
    let no_color = std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty());
    let palette = Palette::new(no_color);

    loop {
        terminal.draw(|frame| view::render(frame, &app, &palette, sizing))?;

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

fn open_tty() -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

/// `Drop` restores the tty on every exit path, including an early `Err`
/// return and an unwind.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<File>>,
}

impl TerminalGuard {
    fn new() -> Result<Self, WrkError> {
        let tty = open_tty().map_err(|_| WrkError::NoTerminal)?;
        // Installed before touching the terminal so a panic during setup
        // itself still restores whatever state `enable_raw_mode` reached.
        set_panic_hook();
        enable_raw_mode()?;
        execute!(tty.try_clone()?, EnterAlternateScreen)?;
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(tty))?,
        })
    }
}

impl std::ops::Deref for TerminalGuard {
    type Target = Terminal<CrosstermBackend<File>>;

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
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}

/// Reopens `/dev/tty` because the panic hook cannot reach the guard's handle.
fn restore_tty() {
    let _ = disable_raw_mode();
    if let Ok(mut tty) = open_tty() {
        let _ = execute!(tty, LeaveAlternateScreen);
    }
}

fn set_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_tty();
        previous(info);
    }));
}
