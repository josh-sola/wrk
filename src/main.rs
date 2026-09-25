use clap::{Parser, Subcommand};
use wrk::paths::Paths;

#[derive(Parser)]
#[command(name = "wrk", about = "Manage git worktrees and their hooks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Clone a repo into the wrk root.
    Clone {
        url: String,
        name: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Create a worktree for a repo.
    New {
        repo: String,
        tree: String,
        /// Check out origin/<tree> instead of branching from origin's default branch.
        #[arg(long)]
        track: bool,
        #[arg(long)]
        wait: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove a worktree.
    Rm {
        repo: String,
        tree: String,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Wait for a tree's hook to finish.
    Wait {
        repo: String,
        tree: String,
        #[arg(long)]
        timeout: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Create or resume a harness session in a worktree.
    Go {
        repo: Option<String>,
        tree: Option<String>,
        harness: Option<String>,
        #[arg(long)]
        new: bool,
        /// Wait for on_create to finish, and don't launch if it failed.
        #[arg(long)]
        wait: bool,
        /// When creating the tree, check out origin/<tree> instead of branching fresh.
        #[arg(long)]
        track: bool,
        /// Fill the whole terminal with the picker instead of a centered panel.
        #[arg(long)]
        fill: bool,
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Open the picker and print the selection without launching anything.
    Pick {
        #[arg(long)]
        json: bool,
        /// Fill the whole terminal instead of drawing a centered panel.
        #[arg(long)]
        fill: bool,
    },
    /// List repos and trees.
    Ls {
        repo: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print or follow a tree's hook log.
    Logs {
        repo: String,
        tree: String,
        #[arg(short = 'f', long)]
        follow: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print a repo's or tree's path.
    Path {
        repo: String,
        tree: Option<String>,
        #[arg(long)]
        json: bool,
    },
    #[command(hide = true, subcommand)]
    Internal(InternalCommand),
}

#[derive(Subcommand)]
enum InternalCommand {
    RunHook {
        repo: String,
        tree: String,
        hook: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let paths = Paths::from_env();

    let code = match cli.command {
        Command::Clone { url, name, json } => {
            wrk::cmd::clone::run(&paths, &url, name.as_deref(), json)
        }
        Command::New {
            repo,
            tree,
            track,
            wait,
            json,
        } => wrk::cmd::new::run(&paths, &repo, &tree, track, wait, json),
        Command::Rm {
            repo,
            tree,
            force,
            json,
        } => wrk::cmd::rm::run(&paths, &repo, &tree, force, json),
        Command::Wait {
            repo,
            tree,
            timeout,
            json,
        } => wrk::cmd::wait::run(&paths, &repo, &tree, timeout, json),
        Command::Go {
            repo,
            tree,
            harness,
            new,
            wait,
            track,
            fill,
            args,
        } => wrk::cmd::go::run(
            &paths,
            repo,
            tree,
            harness,
            wrk::cmd::go::GoFlags {
                new_tree: new,
                wait,
                track,
                fill,
            },
            args,
        ),
        Command::Pick { json, fill } => {
            let sizing = if fill {
                wrk::tui::Sizing::Fill
            } else {
                wrk::tui::Sizing::Centered
            };
            wrk::cmd::pick::run(&paths, json, sizing)
        }
        Command::Ls { repo, json } => wrk::cmd::ls::run(&paths, repo.as_deref(), json),
        Command::Logs {
            repo,
            tree,
            follow,
            json,
        } => wrk::cmd::logs::run(&paths, &repo, &tree, follow, json),
        Command::Path { repo, tree, json } => {
            wrk::cmd::path::run(&paths, &repo, tree.as_deref(), json)
        }
        Command::Internal(InternalCommand::RunHook { repo, tree, hook }) => {
            wrk::cmd::internal::run_hook(&paths, &repo, &tree, &hook)
        }
    };

    std::process::exit(code);
}
