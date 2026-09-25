#!/bin/sh
# Offline coverage for open.sh: fake wrk and herdr binaries record every
# call's argv, and each scenario asserts on that log instead of a live
# herdr session.
# shellcheck disable=SC2089,SC2090
set -eu

SCRIPT_DIR=$(dirname -- "$0")
SCRIPT_DIR=$(cd "$SCRIPT_DIR" && pwd)
OPEN_SH="$SCRIPT_DIR/open.sh"

WORK=$(mktemp -d)
WORK=$(cd "$WORK" && pwd -P)
trap 'rm -rf "$WORK"' EXIT

FAKE_BIN="$WORK/bin"
mkdir -p "$FAKE_BIN"
export WRK_CALL_LOG="$WORK/wrk.log"
export HERDR_CALL_LOG="$WORK/herdr.log"
export WRK_BIN="$FAKE_BIN/wrk"
export HERDR_BIN="$FAKE_BIN/herdr"

cat >"$FAKE_BIN/wrk" <<'FAKE_WRK'
#!/bin/sh
printf '%s\n' "$*" >>"$WRK_CALL_LOG"
case "$1" in
    pick) exit_code=${WRK_PICK_EXIT:-0}; out=${WRK_PICK_OUT:-'{}'} ;;
    new) exit_code=${WRK_NEW_EXIT:-0}; out=${WRK_NEW_OUT:-'{}'} ;;
    path) exit_code=${WRK_PATH_EXIT:-0}; out=${WRK_PATH_OUT:-'{}'} ;;
    *) exit_code=1; out='{"error":{"code":"unknown_command","message":"fake wrk: unhandled"}}' ;;
esac
if [ "$1" = "new" ] && [ "$exit_code" -eq 0 ]; then
    new_path=$(printf '%s' "$out" | jq -r '.path')
    mkdir -p "$new_path"
fi
printf '%s\n' "$out"
exit "$exit_code"
FAKE_WRK
chmod +x "$FAKE_BIN/wrk"

cat >"$FAKE_BIN/herdr" <<'FAKE_HERDR'
#!/bin/sh
printf '%s\n' "$*" >>"$HERDR_CALL_LOG"
case "$1 $2" in
    "worktree list") exit_code=${HERDR_WORKTREE_LIST_EXIT:-0}; out=${HERDR_WORKTREE_LIST_OUT:-'{"result":{"worktrees":[]}}'} ;;
    "workspace list") exit_code=${HERDR_WORKSPACE_LIST_EXIT:-0}; out=${HERDR_WORKSPACE_LIST_OUT:-'{"result":{"workspaces":[]}}'} ;;
    "tab create") exit_code=${HERDR_TAB_CREATE_EXIT:-0}; out=${HERDR_TAB_CREATE_OUT:-'{}'} ;;
    "worktree open") exit_code=${HERDR_WORKTREE_OPEN_EXIT:-0}; out=${HERDR_WORKTREE_OPEN_OUT:-'{}'} ;;
    "workspace create") exit_code=${HERDR_WORKSPACE_CREATE_EXIT:-0}; out=${HERDR_WORKSPACE_CREATE_OUT:-'{}'} ;;
    "pane rename") exit_code=${HERDR_PANE_RENAME_EXIT:-0}; out='{"result":null}' ;;
    "pane run") exit_code=${HERDR_PANE_RUN_EXIT:-0}; out='{"result":null}' ;;
    *) exit_code=1; out='{"error":{"code":"unknown_command","message":"fake herdr: unhandled"}}' ;;
esac
if [ "$exit_code" -ne 0 ]; then
    printf '%s\n' "$out" >&2
else
    printf '%s\n' "$out"
fi
exit "$exit_code"
FAKE_HERDR
chmod +x "$FAKE_BIN/herdr"

FAILED=0
ok() { printf 'ok - %s\n' "$1"; }
bad() { printf 'FAIL - %s\n' "$1"; FAILED=1; }
assert_eq() {
    if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (expected [$2] got [$3])"; fi
}
assert_contains() {
    if grep -qF "$3" "$2" 2>/dev/null; then ok "$1"; else bad "$1 (missing [$3] in $2)"; fi
}
assert_not_contains() {
    if grep -qF "$3" "$2" 2>/dev/null; then bad "$1 (unexpected [$3] found)"; else ok "$1"; fi
}

reset_env() {
    unset WRK_PICK_EXIT WRK_PICK_OUT WRK_NEW_EXIT WRK_NEW_OUT WRK_PATH_EXIT WRK_PATH_OUT \
        HERDR_WORKTREE_LIST_EXIT HERDR_WORKTREE_LIST_OUT HERDR_WORKSPACE_LIST_EXIT HERDR_WORKSPACE_LIST_OUT \
        HERDR_TAB_CREATE_EXIT HERDR_TAB_CREATE_OUT HERDR_WORKTREE_OPEN_EXIT HERDR_WORKTREE_OPEN_OUT \
        HERDR_WORKSPACE_CREATE_EXIT HERDR_WORKSPACE_CREATE_OUT HERDR_PANE_RENAME_EXIT HERDR_PANE_RUN_EXIT \
        2>/dev/null || true
}

mkdir_tree() {
    d="$WORK/trees/$1/$2"
    mkdir -p "$d"
    printf '%s' "$d"
}

run_open() {
    : >"$WRK_CALL_LOG"
    : >"$HERDR_CALL_LOG"
    set +e
    sh "$OPEN_SH" </dev/null >"$WORK/stdout" 2>"$WORK/stderr"
    EXIT_CODE=$?
    set -e
}

echo "== existing tree, open workspace =="
reset_env
TREE=$(mkdir_tree myrepo mytree)
export WRK_PICK_OUT="{\"repo\":\"myrepo\",\"tree\":\"mytree\",\"harness\":\"claude\",\"new\":false,\"path\":\"$TREE\"}"
export HERDR_WORKTREE_LIST_OUT="{\"result\":{\"worktrees\":[{\"path\":\"$TREE\",\"is_linked_worktree\":true,\"open_workspace_id\":\"w5\"}]}}"
export HERDR_TAB_CREATE_OUT='{"result":{"tab":{"tab_id":"w5:t2"},"root_pane":{"pane_id":"w5:p2"}}}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_contains "tab create on the open workspace" "$HERDR_CALL_LOG" "tab create --workspace w5 --cwd $TREE --label claude --focus"
assert_not_contains "no worktree open" "$HERDR_CALL_LOG" "worktree open"
assert_not_contains "no workspace create" "$HERDR_CALL_LOG" "workspace create"
assert_contains "pane renamed" "$HERDR_CALL_LOG" "pane rename w5:p2 claude"
assert_contains "pane run launches the harness" "$HERDR_CALL_LOG" "pane run w5:p2 'wrk' 'go' 'myrepo' 'mytree' 'claude'"

echo "== existing tree, no workspace, a parent workspace =="
reset_env
TREE=$(mkdir_tree myrepo mytree)
export WRK_PICK_OUT="{\"repo\":\"myrepo\",\"tree\":\"mytree\",\"harness\":\"claude\",\"new\":false,\"path\":\"$TREE\"}"
export HERDR_WORKTREE_LIST_OUT="{\"result\":{\"worktrees\":[{\"path\":\"$TREE\",\"is_linked_worktree\":true,\"open_workspace_id\":null},{\"path\":\"/repo-root\",\"is_linked_worktree\":false,\"open_workspace_id\":\"wP\"}]}}"
export HERDR_WORKTREE_OPEN_OUT='{"result":{"tab":{"tab_id":"wP:t2"},"root_pane":{"pane_id":"wP:p2"},"workspace":{"workspace_id":"wP"},"already_open":false}}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_contains "worktree open on the parent workspace" "$HERDR_CALL_LOG" "worktree open --workspace wP --path $TREE --label mytree --focus"
assert_not_contains "no tab create" "$HERDR_CALL_LOG" "tab create"
assert_not_contains "no workspace create" "$HERDR_CALL_LOG" "workspace create"
assert_contains "pane renamed" "$HERDR_CALL_LOG" "pane rename wP:p2 claude"

echo "== no parent workspace =="
reset_env
TREE=$(mkdir_tree myrepo mytree)
REPO_CHECKOUT="$WORK/repos/myrepo"
mkdir -p "$REPO_CHECKOUT"
export WRK_PICK_OUT="{\"repo\":\"myrepo\",\"tree\":\"mytree\",\"harness\":\"claude\",\"new\":false,\"path\":\"$TREE\"}"
export HERDR_WORKTREE_LIST_OUT="{\"result\":{\"worktrees\":[{\"path\":\"$TREE\",\"is_linked_worktree\":true,\"open_workspace_id\":null},{\"path\":\"$REPO_CHECKOUT\",\"is_linked_worktree\":false,\"open_workspace_id\":null}]}}"
export WRK_PATH_OUT="{\"path\":\"$REPO_CHECKOUT\"}"
export HERDR_WORKSPACE_CREATE_OUT='{"result":{"workspace":{"workspace_id":"wNew"}}}'
export HERDR_WORKTREE_OPEN_OUT='{"result":{"tab":{"tab_id":"wNew:t2"},"root_pane":{"pane_id":"wNew:p2"},"workspace":{"workspace_id":"wNew"},"already_open":false}}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_contains "wrk path looked up the repo checkout" "$WRK_CALL_LOG" "path myrepo --json"
assert_contains "parent workspace created without focus" "$HERDR_CALL_LOG" "workspace create --cwd $REPO_CHECKOUT --label myrepo --no-focus"
assert_contains "worktree opened on the new parent" "$HERDR_CALL_LOG" "worktree open --workspace wNew --path $TREE --label mytree --focus"

echo "== worktree open fails, falls back to a plain workspace =="
reset_env
TREE=$(mkdir_tree myrepo mytree)
export WRK_PICK_OUT="{\"repo\":\"myrepo\",\"tree\":\"mytree\",\"harness\":\"claude\",\"new\":false,\"path\":\"$TREE\"}"
export HERDR_WORKTREE_LIST_OUT="{\"result\":{\"worktrees\":[{\"path\":\"$TREE\",\"is_linked_worktree\":true,\"open_workspace_id\":null},{\"path\":\"/repo-root\",\"is_linked_worktree\":false,\"open_workspace_id\":\"wP\"}]}}"
export HERDR_WORKTREE_OPEN_EXIT=1
export HERDR_WORKTREE_OPEN_OUT='{"error":{"code":"already_open","message":"worktree is already open"}}'
export HERDR_WORKSPACE_CREATE_OUT='{"result":{"workspace":{"workspace_id":"wFallback"},"root_pane":{"pane_id":"wFallback:p1"}}}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_contains "worktree open was attempted" "$HERDR_CALL_LOG" "worktree open --workspace wP --path $TREE --label mytree --focus"
assert_contains "fallback workspace create, focused" "$HERDR_CALL_LOG" "workspace create --cwd $TREE --label mytree --focus"
assert_contains "harness launched on the fallback pane" "$HERDR_CALL_LOG" "pane rename wFallback:p1 claude"

echo "== a new tree =="
reset_env
TREE="$WORK/trees/myrepo/newtree"
export WRK_PICK_OUT="{\"repo\":\"myrepo\",\"tree\":\"newtree\",\"harness\":\"claude\",\"new\":true,\"path\":\"$TREE\"}"
export WRK_NEW_OUT="{\"repo\":\"myrepo\",\"tree\":\"newtree\",\"path\":\"$TREE\",\"branch\":\"newtree\",\"hook_status\":\"running\"}"
export HERDR_WORKTREE_LIST_OUT='{"result":{"worktrees":[{"path":"/repo-root","is_linked_worktree":false,"open_workspace_id":"wP"}]}}'
export HERDR_WORKTREE_OPEN_OUT='{"result":{"tab":{"tab_id":"wP:t3"},"root_pane":{"pane_id":"wP:p3"},"workspace":{"workspace_id":"wP"},"already_open":false}}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_contains "wrk new ran before placement" "$WRK_CALL_LOG" "new myrepo newtree --json"
assert_contains "harness launched without --new" "$HERDR_CALL_LOG" "pane run wP:p3 'wrk' 'go' 'myrepo' 'newtree' 'claude'"
assert_not_contains "no --new on the launch line" "$HERDR_CALL_LOG" "--new"

echo "== cancel =="
reset_env
export WRK_PICK_EXIT=130
export WRK_PICK_OUT='{"cancelled":true}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_eq "no herdr calls" "" "$(cat "$HERDR_CALL_LOG")"

echo "== pick error =="
reset_env
export WRK_PICK_EXIT=1
export WRK_PICK_OUT='{"error":{"code":"no_terminal","message":"wrk pick needs a terminal"}}'
run_open
if [ "$EXIT_CODE" -ne 0 ]; then ok "non-zero exit"; else bad "non-zero exit (got $EXIT_CODE)"; fi
assert_contains "error message shown" "$WORK/stderr" "wrk pick needs a terminal"
assert_eq "no herdr calls" "" "$(cat "$HERDR_CALL_LOG")"

echo "== tree name with a single quote =="
reset_env
TREE=$(mkdir_tree myrepo "fix's-bug")
export WRK_PICK_OUT="{\"repo\":\"myrepo\",\"tree\":\"fix's-bug\",\"harness\":\"claude\",\"new\":false,\"path\":\"$TREE\"}"
export HERDR_WORKTREE_LIST_OUT="{\"result\":{\"worktrees\":[{\"path\":\"$TREE\",\"is_linked_worktree\":true,\"open_workspace_id\":\"w7\"}]}}"
export HERDR_TAB_CREATE_OUT='{"result":{"tab":{"tab_id":"w7:t2"},"root_pane":{"pane_id":"w7:p2"}}}'
run_open
assert_eq "exit 0" 0 "$EXIT_CODE"
assert_contains "quote is escaped in the launch line" "$HERDR_CALL_LOG" "pane run w7:p2 'wrk' 'go' 'myrepo' 'fix'\\''s-bug' 'claude'"

if [ "$FAILED" -eq 0 ]; then
    echo "all scenarios passed"
else
    echo "some scenarios FAILED"
fi
exit "$FAILED"
