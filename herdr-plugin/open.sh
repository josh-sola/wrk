#!/bin/sh
# Places the harness `wrk go` would otherwise launch in the calling
# terminal into a herdr workspace instead.
set -eu

WRK_BIN=${WRK_BIN:-wrk}
HERDR_BIN=${HERDR_BIN:-${HERDR_BIN_PATH:-herdr}}

# herdr closes a plugin pane the instant its process exits, so an error
# printed here would otherwise vanish unread.
pause_for_close() {
    if [ -n "${HERDR_PLUGIN_ENTRYPOINT_ID:-}" ] && [ -t 0 ]; then
        printf 'press Enter to close\n' >&2
        # shellcheck disable=SC2034
        read -r _discard || true
    fi
}

die() {
    printf 'error: %s\n' "$1" >&2
    pause_for_close
    exit 1
}

die_from_json() {
    msg=$(printf '%s' "$2" | jq -r '.error.message // empty' 2>/dev/null || true)
    if [ -n "$msg" ]; then
        die "$1 failed: $msg"
    fi
    die "$1 failed: $2"
}

# Runs $WRK_BIN "$@" without dying, so a cancel (exit 130) can be told apart
# from a real failure. Sets WRK_STATUS and WRK_OUT.
run_wrk() {
    WRK_STATUS=0
    WRK_OUT=$("$WRK_BIN" "$@") || WRK_STATUS=$?
}

wrk_or_die() {
    run_wrk "$@"
    [ "$WRK_STATUS" -eq 0 ] || die_from_json "wrk $*" "$WRK_OUT"
}

# Runs $HERDR_BIN "$@" without dying, so a fallback path can inspect the
# failure. Sets HERDR_STATUS, HERDR_OUT and, on success, HERDR_RESULT.
run_herdr() {
    HERDR_STATUS=0
    HERDR_OUT=$("$HERDR_BIN" "$@" 2>&1) || HERDR_STATUS=$?
    HERDR_RESULT=""
    if [ "$HERDR_STATUS" -eq 0 ]; then
        HERDR_RESULT=$(printf '%s' "$HERDR_OUT" | jq -c '.result' 2>/dev/null) || true
    fi
}

herdr_or_die() {
    run_herdr "$@"
    [ "$HERDR_STATUS" -eq 0 ] || die_from_json "herdr $*" "$HERDR_OUT"
}

canonical_path() {
    (cd "$1" 2>/dev/null && pwd -P) || true
}

# `herdr pane run` sends its argument as one shell line, so each token is
# quoted here rather than left to the CLI.
shell_quote() {
    printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

shell_line() {
    line=
    for tok in "$@"; do
        line="$line $(shell_quote "$tok")"
    done
    printf '%s' "${line# }"
}

place() {
    repo=$1
    tree=$2
    harness=$3
    canon=$4

    herdr_or_die worktree list --cwd "$canon"
    worktrees=$HERDR_RESULT
    workspace_id=$(printf '%s' "$worktrees" | jq -r --arg p "$canon" \
        '[.worktrees[] | select(.path == $p) | .open_workspace_id] | map(select(. != null)) | .[0] // empty')

    if [ -z "$workspace_id" ]; then
        herdr_or_die workspace list
        workspace_id=$(printf '%s' "$HERDR_RESULT" | jq -r --arg p "$canon" \
            '[.workspaces[] | select(.worktree.checkout_path == $p) | .workspace_id] | .[0] // empty')
    fi

    if [ -n "$workspace_id" ]; then
        herdr_or_die tab create --workspace "$workspace_id" --cwd "$canon" --label "$harness" --focus
        pane_id=$(printf '%s' "$HERDR_RESULT" | jq -r '.root_pane.pane_id')
    else
        parent_id=$(printf '%s' "$worktrees" | jq -r \
            '[.worktrees[] | select(.is_linked_worktree == false) | .open_workspace_id] | map(select(. != null)) | .[0] // empty')
        if [ -z "$parent_id" ]; then
            wrk_or_die path "$repo" --json
            repo_checkout=$(printf '%s' "$WRK_OUT" | jq -r '.path')
            herdr_or_die workspace create --cwd "$repo_checkout" --label "$repo" --no-focus
            parent_id=$(printf '%s' "$HERDR_RESULT" | jq -r '.workspace.workspace_id')
        fi

        run_herdr worktree open --workspace "$parent_id" --path "$canon" --label "$tree" --focus
        if [ "$HERDR_STATUS" -eq 0 ]; then
            pane_id=$(printf '%s' "$HERDR_RESULT" | jq -r '.root_pane.pane_id')
        else
            printf 'herdr worktree open failed; opening a plain workspace instead\n' >&2
            herdr_or_die workspace create --cwd "$canon" --label "$tree" --focus
            pane_id=$(printf '%s' "$HERDR_RESULT" | jq -r '.root_pane.pane_id')
        fi
    fi

    herdr_or_die pane rename "$pane_id" "$harness"
    herdr_or_die pane run "$pane_id" "$(shell_line wrk go "$repo" "$tree" "$harness")"
}

main() {
    command -v jq >/dev/null 2>&1 || die "open.sh needs jq on PATH"

    run_wrk pick --json --fill
    if [ "$WRK_STATUS" -eq 130 ] || printf '%s' "$WRK_OUT" | jq -e '.cancelled == true' >/dev/null 2>&1; then
        exit 0
    fi
    [ "$WRK_STATUS" -eq 0 ] || die_from_json "wrk pick" "$WRK_OUT"

    pick=$WRK_OUT
    repo=$(printf '%s' "$pick" | jq -r '.repo')
    tree=$(printf '%s' "$pick" | jq -r '.tree')
    harness=$(printf '%s' "$pick" | jq -r '.harness')
    new=$(printf '%s' "$pick" | jq -r '.new')
    path=$(printf '%s' "$pick" | jq -r '.path')

    if [ "$new" = "true" ]; then
        wrk_or_die new "$repo" "$tree" --json
    fi

    canon=$(canonical_path "$path")
    [ -n "$canon" ] || die "tree path does not exist: $path"
    place "$repo" "$tree" "$harness" "$canon"
}

main "$@"
