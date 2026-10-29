#!/usr/bin/env bash
# Installs rw, its Rex actions, and the rw-launch composer.
# Files are copied, not linked, because trees get deleted.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

mkdir -p "$HOME/.local/bin" "$HOME/.config/rex"
install -m 755 "$here/rw" "$HOME/.local/bin/rw"
install -m 644 "$here/rw.lua" "$HOME/.config/rex/rw.lua"
cargo install --path "$here/launch" --locked
rex config reload
