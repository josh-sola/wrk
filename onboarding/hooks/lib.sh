# Shared helpers for on_create hooks. Source it; don't run it.
set -eu

# Hooks can start from a bare environment (a GUI launcher or a popup pane),
# so put the usual tool locations first.
export PATH="$HOME/.local/share/mise/shims:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

step() {
    echo "==> $*"
    "$@"
}

# Every tree of a repo shares one plans folder that survives `wrk rm`.
link_plans() {
    plans="$WRK_ROOT/plans/$WRK_REPO"
    mkdir -p "$plans"

    link="$WRK_TREE_PATH/plans"
    if [ -e "$link" ] || [ -L "$link" ]; then
        echo "==> plans/ already exists in the tree; leaving it alone"
    else
        ln -s "$plans" "$link"
        echo "==> linked plans/ -> $plans"
    fi

    # `/plans` without a trailing slash: git sees a symlink as a file, so
    # `plans/` would not match it. The common exclude file covers every tree.
    exclude="$(git -C "$WRK_TREE_PATH" rev-parse --path-format=absolute --git-common-dir)/info/exclude"
    mkdir -p "$(dirname "$exclude")"
    grep -qxF '/plans' "$exclude" 2>/dev/null || echo '/plans' >> "$exclude"
}
