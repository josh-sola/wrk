//! Manual try-out for the `wrk go` picker: `cargo run --example pick_demo`.
use wrk::tui::{PickInput, Sizing, TreeRow, pick};

fn main() {
    let input = PickInput {
        trees: vec![
            TreeRow {
                repo: "myproj".to_string(),
                tree: "feature-a".to_string(),
                status: "succeeded".to_string(),
            },
            TreeRow {
                repo: "myproj".to_string(),
                tree: "feature-b".to_string(),
                status: "pending".to_string(),
            },
            TreeRow {
                repo: "myproj".to_string(),
                tree: "old-experiment".to_string(),
                status: "none".to_string(),
            },
            TreeRow {
                repo: "api".to_string(),
                tree: "feat-login".to_string(),
                status: "failed".to_string(),
            },
            TreeRow {
                repo: "api".to_string(),
                tree: "hotfix".to_string(),
                status: "running".to_string(),
            },
            TreeRow {
                repo: "api".to_string(),
                tree: "stale-branch".to_string(),
                status: "crashed".to_string(),
            },
        ],
        repos: vec!["myproj".to_string(), "api".to_string()],
        harnesses: vec![
            "claude".to_string(),
            "codex".to_string(),
            "devin".to_string(),
            "pi".to_string(),
        ],
        default_harness: Some("claude".to_string()),
    };

    let sizing = if std::env::args().any(|a| a == "--fill") {
        Sizing::Fill
    } else {
        Sizing::Centered
    };
    match pick(input, sizing) {
        Ok(Some(target)) => println!("picked: {target:?}"),
        Ok(None) => println!("cancelled"),
        Err(err) => eprintln!("error: {err}"),
    }
}
