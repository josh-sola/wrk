//! `rw-launch`: a popup that composes a new `rw` session.
mod layout;
mod state;
mod theme;
mod view;

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use wrk::tui::style::Palette;

use state::{AGENTS, App, Request, Step, Ticket};
use theme::Theme;

#[derive(Default, Serialize, Deserialize)]
struct Remembered {
    repo: Option<String>,
    agent: Option<String>,
}

fn remembered_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/state/rw/launch.json"))
}

fn load_remembered() -> Remembered {
    remembered_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_remembered(request: &Request) {
    let Some(path) = remembered_path() else {
        return;
    };
    let value = Remembered {
        repo: Some(request.repo.clone()),
        agent: Some(request.agent.to_string()),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string_pretty(&value) {
        let _ = std::fs::write(path, text);
    }
}

fn load_repos() -> Result<Vec<String>, String> {
    let out = Command::new("wrk")
        .args(["ls", "--json"])
        .output()
        .map_err(|e| format!("wrk: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "wrk ls failed: {}",
            String::from_utf8_lossy(&out.stdout)
        ));
    }
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let mut repos: Vec<String> = doc["repos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["name"].as_str().map(str::to_string))
        .collect();
    repos.sort_by_key(|r| (r != "monorepo", r.clone()));
    if repos.is_empty() {
        return Err("wrk has no repos".to_string());
    }
    Ok(repos)
}

fn tickets_cache() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/state/rw/tickets.json"))
}

fn load_cached_tickets() -> Option<Vec<Ticket>> {
    let text = std::fs::read_to_string(tickets_cache()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_cached_tickets(tickets: &[Ticket]) {
    let Some(path) = tickets_cache() else {
        return;
    };
    let Ok(text) = serde_json::to_string(tickets) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

fn fetch_tickets() -> Result<Vec<Ticket>, String> {
    let out = Command::new("rw")
        .args(["tickets", "--json"])
        .output()
        .map_err(|e| format!("rw: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err
            .trim()
            .lines()
            .last()
            .unwrap_or("rw tickets failed")
            .to_string());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("bad ticket list: {e}"))
}

fn open_tty() -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

/// `Drop` restores the tty on every exit path, including an early `Err`
/// return and an unwind.
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<File>>,
}

impl TerminalGuard {
    fn new() -> io::Result<Self> {
        let tty = open_tty()?;
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            if let Ok(mut tty) = open_tty() {
                let _ = execute!(tty, DisableMouseCapture, LeaveAlternateScreen);
            }
            previous(info);
        }));
        enable_raw_mode()?;
        execute!(tty.try_clone()?, EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Self {
            terminal: Terminal::new(CrosstermBackend::new(tty))?,
        })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}

/// Asks the terminal for its background and foreground colors. Gives up
/// after about 150 ms in total, so a terminal that never answers can't hang it.
fn query_theme() -> Theme {
    let mut buf = Vec::new();
    if let Ok(mut tty) = open_tty()
        && enable_raw_mode().is_ok()
    {
        if tty.write_all(b"\x1b]11;?\x1b\\\x1b]10;?\x1b\\").is_ok() {
            read_replies(&tty, &mut buf);
        }
        let _ = disable_raw_mode();
    }
    let (background, foreground) = theme::parse_replies(&buf);
    if std::env::var_os("RW_LAUNCH_DEBUG").is_some() {
        eprintln!("rw-launch: background {background:?} foreground {foreground:?}");
    }
    Theme::from_terminal(background, foreground)
}

fn read_replies(tty: &File, buf: &mut Vec<u8>) {
    let deadline = Instant::now() + Duration::from_millis(150);
    let mut chunk = [0u8; 256];
    while {
        let (background, foreground) = theme::parse_replies(buf);
        background.is_none() || foreground.is_none()
    } {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        let mut fds = libc::pollfd {
            fd: tty.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `fds` is a valid pollfd and the count is 1.
        let ready = unsafe { libc::poll(&mut fds, 1, left.as_millis() as i32) };
        if ready <= 0 {
            return;
        }
        // SAFETY: `chunk` is a valid buffer of the length passed.
        let n = unsafe { libc::read(fds.fd, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n <= 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
    }
}

/// Draws until the user starts a session or quits. A layer can be drawn
/// before its pane has a size, so each pass redraws and waits briefly.
fn compose(mut app: App, started: Instant) -> io::Result<Option<Request>> {
    if let Some(cached) = load_cached_tickets() {
        app.set_tickets(Ok(cached));
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = fetch_tickets();
        if let Ok(tickets) = &result {
            save_cached_tickets(tickets);
        }
        let _ = tx.send(result);
    });

    let theme = query_theme();
    let mut guard = TerminalGuard::new()?;
    let palette = Palette::new(std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()));
    loop {
        if let Ok(result) = rx.try_recv() {
            app.set_tickets(result);
        }
        guard
            .terminal
            .draw(|frame| view::render(frame, &app, &palette, &theme))?;
        if std::env::var_os("RW_LAUNCH_FIRST_FRAME").is_some() {
            eprintln!("rw-launch: first frame after {:?}", started.elapsed());
            return Ok(None);
        }
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let step = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle(key),
            Event::Mouse(mouse) => {
                let size = guard.terminal.size()?;
                app.handle_mouse(mouse, Rect::new(0, 0, size.width, size.height))
            }
            _ => continue,
        };
        match step {
            Step::Continue => {}
            Step::Start(request) => return Ok(Some(request)),
            Step::Quit => return Ok(None),
        }
    }
}

fn run_rw(request: &Request, client: Option<&str>) -> bool {
    let mut cmd = Command::new("rw");
    cmd.args(["new", &request.name, "--branch", &request.branch]);
    cmd.args(["--repo", &request.repo, "--agent", request.agent]);
    if let Some(client) = client.filter(|c| !c.is_empty()) {
        cmd.args(["--client", client]);
    }
    cmd.arg("--json");
    println!("starting {} in {}…", request.name, request.repo);
    match cmd.output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            eprintln!("{}", String::from_utf8_lossy(&out.stderr).trim_end());
            false
        }
        Err(e) => {
            eprintln!("rw: {e}");
            false
        }
    }
}

fn wait_for_enter() {
    print!("\nrw failed. Press Enter to close.");
    let _ = io::stdout().flush();
    let _ = io::stdin().lock().read_line(&mut String::new());
}

fn main() -> ExitCode {
    let started = Instant::now();
    let mut client = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--client" => client = args.next(),
            "--session" | "--block" => {
                args.next();
            }
            other => {
                eprintln!("rw-launch: unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }

    let repos = match load_repos() {
        Ok(repos) => repos,
        Err(message) => {
            eprintln!("rw-launch: {message}");
            wait_for_enter();
            return ExitCode::FAILURE;
        }
    };
    let last = load_remembered();
    let repo = last
        .repo
        .and_then(|r| repos.iter().position(|x| *x == r))
        .unwrap_or(0);
    let agent = AGENTS
        .iter()
        .position(|a| Some(*a) == last.agent.as_deref())
        .unwrap_or(0);

    let request = match compose(App::new(repos, repo, agent), started) {
        Ok(Some(request)) => request,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rw-launch: {e}");
            wait_for_enter();
            return ExitCode::FAILURE;
        }
    };
    save_remembered(&request);
    if run_rw(&request, client.as_deref()) {
        ExitCode::SUCCESS
    } else {
        wait_for_enter();
        ExitCode::FAILURE
    }
}
