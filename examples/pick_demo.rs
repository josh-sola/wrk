//! Manual try-out for the `wrk go` picker: `cargo run --example pick_demo`.
use wrk::tui::{PickInput, TreeRow, pick};

fn main() {
    let input = PickInput {
        trees: vec![
            TreeRow {
                repo: "wrk".to_string(),
                tree: "feature".to_string(),
                status: "succeeded".to_string(),
            },
            TreeRow {
                repo: "wrk".to_string(),
                tree: "bugfix".to_string(),
                status: "running".to_string(),
            },
            TreeRow {
                repo: "other".to_string(),
                tree: "main".to_string(),
                status: "none".to_string(),
            },
        ],
        repos: vec!["wrk".to_string(), "other".to_string()],
        harnesses: vec!["claude".to_string(), "codex".to_string(), "pi".to_string()],
        default_harness: Some("codex".to_string()),
    };

    match pick(input) {
        Ok(Some(target)) => println!("picked: {target:?}"),
        Ok(None) => println!("cancelled"),
        Err(err) => eprintln!("error: {err}"),
    }
}
