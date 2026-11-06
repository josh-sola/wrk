//! The close card: confirms closing the current rw session, then runs
//! `rw close`. The state is a pure key-to-step machine, like `state.rs`.
use std::io;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Wrap};
use serde::Deserialize;
use wrk::tui::style::{Palette, truncate};

use crate::theme::Theme;
use crate::view::{border_color, repo_style};
use crate::{TerminalGuard, query_theme};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Target {
    pub label: String,
    pub repo: String,
    pub tree: String,
    #[serde(default)]
    pub child: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Planning,
    Confirm,
    Closing,
    PlanFailed(String),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    Quit,
    Close { force: bool },
}

pub struct CloseApp {
    pub phase: Phase,
    pub targets: Vec<Target>,
}

impl CloseApp {
    pub fn new() -> Self {
        CloseApp {
            phase: Phase::Planning,
            targets: Vec::new(),
        }
    }

    pub fn set_plan(&mut self, plan: Result<Vec<Target>, String>) {
        match plan {
            Ok(targets) => {
                self.targets = targets;
                self.phase = Phase::Confirm;
            }
            Err(message) => self.phase = Phase::PlanFailed(message),
        }
    }

    pub fn set_closed(&mut self, result: Result<(), String>) -> Step {
        match result {
            Ok(()) => Step::Quit,
            Err(message) => {
                self.phase = Phase::Failed(message);
                Step::Continue
            }
        }
    }

    /// The session's own repo: the last target, since children come first.
    pub fn repo(&self) -> &str {
        self.targets.last().map_or("", |t| t.repo.as_str())
    }

    fn child_count(&self) -> usize {
        self.targets.iter().filter(|t| t.child).count()
    }

    pub fn handle(&mut self, key: KeyEvent) -> Step {
        let interrupt =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        match (&self.phase, key.code) {
            (_, _) if interrupt => Step::Quit,
            (Phase::Closing, _) => Step::Continue,
            (Phase::Confirm, KeyCode::Char('y' | 'Y')) => {
                self.phase = Phase::Closing;
                Step::Close { force: false }
            }
            (Phase::Confirm, KeyCode::Char('n' | 'N')) => Step::Quit,
            (Phase::Failed(_), KeyCode::Char('f' | 'F')) => {
                self.phase = Phase::Closing;
                Step::Close { force: true }
            }
            (_, KeyCode::Esc) => Step::Quit,
            _ => Step::Continue,
        }
    }

    fn hints(&self) -> &'static str {
        match self.phase {
            Phase::Confirm => "y close  n/esc cancel",
            Phase::Failed(_) => "f force remove  esc close",
            Phase::Closing => "",
            Phase::Planning | Phase::PlanFailed(_) => "esc close",
        }
    }
}

/// Reads the `{"error": ...}` reply `rw` prints on failure.
fn error_message(doc: &serde_json::Value, fallback: &str) -> String {
    doc["error"].as_str().unwrap_or(fallback).to_string()
}

pub fn parse_plan(stdout: &[u8], success: bool) -> Result<Vec<Target>, String> {
    let doc: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|e| format!("bad close plan: {e}"))?;
    if !success || doc.get("error").is_some() {
        return Err(error_message(&doc, "rw close failed"));
    }
    let targets: Vec<Target> = serde_json::from_value(doc["targets"].clone())
        .map_err(|e| format!("bad close plan: {e}"))?;
    if targets.is_empty() {
        return Err("nothing to close".to_string());
    }
    Ok(targets)
}

pub fn parse_close(stdout: &[u8], success: bool) -> Result<(), String> {
    let doc: serde_json::Value =
        serde_json::from_slice(stdout).map_err(|e| format!("bad close reply: {e}"))?;
    if success && doc.get("error").is_none() {
        return Ok(());
    }
    let mut message = error_message(&doc, "rw close failed");
    let closed: Vec<&str> = doc["closed"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .collect();
    if !closed.is_empty() {
        message.push_str(&format!(" (already closed: {})", closed.join(", ")));
    }
    Err(message)
}

fn run_plan(session: &str) -> Result<Vec<Target>, String> {
    let out = Command::new("rw")
        .args(["close", "--session", session, "--plan", "--json"])
        .output()
        .map_err(|e| format!("rw: {e}"))?;
    parse_plan(&out.stdout, out.status.success())
}

fn run_close(session: &str, force: bool) -> Result<(), String> {
    let mut cmd = Command::new("rw");
    cmd.args(["close", "--session", session, "--with-children", "--json"]);
    if force {
        cmd.arg("--force");
    }
    let out = cmd.output().map_err(|e| format!("rw: {e}"))?;
    parse_close(&out.stdout, out.status.success())
}

enum Msg {
    Plan(Result<Vec<Target>, String>),
    Closed(Result<(), String>),
}

/// Runs the card until the user cancels or the close succeeds. A successful
/// close usually kills the session this popup lives in, so the process may
/// end before this returns.
pub fn run(session: &str, started: Instant) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    {
        let tx = tx.clone();
        let session = session.to_string();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Plan(run_plan(&session)));
        });
    }

    let theme = query_theme();
    let mut guard = TerminalGuard::new()?;
    let palette = Palette::new(std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()));
    let mut app = CloseApp::new();
    loop {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Plan(plan) => app.set_plan(plan),
                Msg::Closed(result) => {
                    if app.set_closed(result) == Step::Quit {
                        return Ok(());
                    }
                }
            }
        }
        guard
            .terminal
            .draw(|frame| render(frame, &app, &palette, &theme))?;
        if std::env::var_os("RW_LAUNCH_FIRST_FRAME").is_some() {
            eprintln!("rw-launch: first frame after {:?}", started.elapsed());
            return Ok(());
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle(key) {
            Step::Continue => {}
            Step::Quit => return Ok(()),
            Step::Close { force } => {
                let tx = tx.clone();
                let session = session.to_string();
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Closed(run_close(&session, force)));
                });
            }
        }
    }
}

fn chip(text: &str) -> String {
    format!(" {text} ")
}

fn target_lines(app: &CloseApp, palette: &Palette, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for target in &app.targets {
        let place = format!("{}/{}", target.repo, target.tree);
        let mut spans = vec![Span::styled(
            truncate(&target.label, width.saturating_sub(2)),
            Style::new().add_modifier(Modifier::BOLD),
        )];
        if target.child {
            spans.push(Span::styled(" child", palette.yellow()));
        }
        lines.push(Line::from(spans));
        lines.push(Line::from(Span::styled(
            truncate(&place, width),
            palette.dim(),
        )));
    }
    lines.push(Line::default());
    let note = match app.child_count() {
        0 => "This removes the worktree and closes the session.".to_string(),
        1 => "Its 1 child session will be closed too.".to_string(),
        n => format!("Its {n} child sessions will be closed too."),
    };
    lines.push(Line::from(note));
    lines
}

pub fn render(frame: &mut Frame, app: &CloseApp, palette: &Palette, theme: &Theme) {
    let area = frame.area();
    if area.width < 24 || area.height < 6 {
        return;
    }
    frame.render_widget(Block::new().style(Style::new().bg(theme.card)), area);
    let card = Rect::new(area.x, area.y, area.width, area.height - 1);
    let footer = Rect::new(area.x, card.bottom(), area.width, 1);

    let repo = app.repo();
    let title = palette.accent_bold().add_modifier(Modifier::REVERSED);
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(
            Style::new()
                .fg(if repo.is_empty() {
                    theme.edge
                } else {
                    border_color(palette, theme, repo)
                })
                .bg(theme.card),
        )
        .padding(Padding::horizontal(1))
        .style(Style::new().bg(theme.card))
        .title_top(Line::from(Span::styled(chip("close"), title)).left_aligned());
    if !repo.is_empty() {
        let style = repo_style(palette, repo).add_modifier(Modifier::REVERSED);
        block = block.title_top(Line::from(Span::styled(chip(repo), style)).right_aligned());
    }
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let width = inner.width as usize;
    let lines = match &app.phase {
        Phase::Planning => vec![Line::from(Span::styled("loading…", palette.dim()))],
        Phase::Confirm => target_lines(app, palette, width),
        Phase::Closing => vec![Line::from(Span::styled("closing…", palette.dim()))],
        Phase::PlanFailed(message) | Phase::Failed(message) => {
            vec![Line::from(Span::styled(message.clone(), palette.red()))]
        }
    };
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(Style::new().bg(theme.card)),
        inner,
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", app.hints()),
            palette.dim(),
        )))
        .style(Style::new().bg(theme.card)),
        footer,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn target(label: &str, child: bool) -> Target {
        Target {
            label: label.to_string(),
            repo: "monorepo".to_string(),
            tree: label.to_lowercase(),
            child,
        }
    }

    fn ready() -> CloseApp {
        let mut app = CloseApp::new();
        app.set_plan(Ok(vec![target("ENG-2", true), target("ENG-1", false)]));
        app
    }

    #[test]
    fn y_starts_the_close_without_force() {
        let mut app = ready();
        assert_eq!(
            app.handle(key(KeyCode::Char('y'))),
            Step::Close { force: false }
        );
        assert_eq!(app.phase, Phase::Closing);
    }

    #[test]
    fn n_and_esc_cancel() {
        assert_eq!(ready().handle(key(KeyCode::Char('n'))), Step::Quit);
        assert_eq!(ready().handle(key(KeyCode::Esc)), Step::Quit);
    }

    #[test]
    fn other_keys_do_nothing_while_confirming() {
        let mut app = ready();
        assert_eq!(app.handle(key(KeyCode::Enter)), Step::Continue);
        assert_eq!(app.handle(key(KeyCode::Char('f'))), Step::Continue);
        assert_eq!(app.phase, Phase::Confirm);
    }

    #[test]
    fn y_is_ignored_before_the_plan_arrives() {
        let mut app = CloseApp::new();
        assert_eq!(app.handle(key(KeyCode::Char('y'))), Step::Continue);
        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Quit);
    }

    #[test]
    fn keys_are_ignored_while_closing() {
        let mut app = ready();
        app.handle(key(KeyCode::Char('y')));
        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Continue);
        assert_eq!(app.handle(key(KeyCode::Char('y'))), Step::Continue);
    }

    #[test]
    fn a_failed_close_offers_force_and_esc() {
        let mut app = ready();
        app.handle(key(KeyCode::Char('y')));
        assert_eq!(app.set_closed(Err("dirty".into())), Step::Continue);
        assert_eq!(app.phase, Phase::Failed("dirty".into()));
        assert_eq!(app.handle(key(KeyCode::Char('y'))), Step::Continue);
        assert_eq!(
            app.handle(key(KeyCode::Char('f'))),
            Step::Close { force: true }
        );
        assert_eq!(app.phase, Phase::Closing);
    }

    #[test]
    fn a_failed_close_closes_on_esc() {
        let mut app = ready();
        app.set_closed(Err("dirty".into()));
        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Quit);
    }

    #[test]
    fn a_successful_close_quits() {
        let mut app = ready();
        assert_eq!(app.set_closed(Ok(())), Step::Quit);
    }

    #[test]
    fn a_failed_plan_only_offers_esc() {
        let mut app = CloseApp::new();
        app.set_plan(Err("no session".into()));
        assert_eq!(app.handle(key(KeyCode::Char('y'))), Step::Continue);
        assert_eq!(app.handle(key(KeyCode::Char('f'))), Step::Continue);
        assert_eq!(app.handle(key(KeyCode::Esc)), Step::Quit);
    }

    #[test]
    fn ctrl_c_quits_in_any_phase() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut app = ready();
        app.handle(key(KeyCode::Char('y')));
        assert_eq!(app.handle(ctrl_c), Step::Quit);
    }

    #[test]
    fn the_session_is_the_last_target() {
        let mut app = ready();
        assert_eq!(app.repo(), "monorepo");
        assert_eq!(app.child_count(), 1);
        app.targets.clear();
        assert_eq!(app.repo(), "");
    }

    #[test]
    fn parses_a_plan() {
        let json = br#"{"targets":[{"label":"A","repo":"r","tree":"t","child":true},{"label":"B","repo":"r","tree":"u"}]}"#;
        let plan = parse_plan(json, true).unwrap();
        assert_eq!(plan.len(), 2);
        assert!(plan[0].child);
        assert!(!plan[1].child);
    }

    #[test]
    fn a_plan_error_carries_the_message() {
        assert_eq!(
            parse_plan(br#"{"error":"not an rw session"}"#, false),
            Err("not an rw session".to_string())
        );
        assert!(parse_plan(b"not json", true).is_err());
        assert!(parse_plan(br#"{"targets":[]}"#, true).is_err());
    }

    #[test]
    fn parses_close_replies() {
        assert_eq!(parse_close(br#"{"closed":["A"]}"#, true), Ok(()));
        assert_eq!(
            parse_close(br#"{"error":"dirty tree","closed":["A","B"]}"#, false),
            Err("dirty tree (already closed: A, B)".to_string())
        );
        assert_eq!(
            parse_close(br#"{"error":"dirty tree"}"#, false),
            Err("dirty tree".to_string())
        );
    }
}
