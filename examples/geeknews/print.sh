#!/usr/bin/env bash
# Fetch recent GeekNews entries, render to a 1-bit PNG, and print via nemonic-cli.
# Usage: ./print.sh [count] [start]
set -euo pipefail

count="${1:-2}"
start="${2:-0}"

script_dir="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cli="${NEMONIC_CLI:-$repo_root/target/release/nemonic-cli}"
serial="${NEMONIC_SERIAL:-}"
out="${OUT:-$script_dir/geeknews.png}"
font="${FONT:-/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc}"

python3 "$script_dir/render.py" --start "$start" --count "$count" --font "$font" -o "$out"

cmd=(sudo "$cli")
if [[ -n "$serial" ]]; then
  cmd+=(--serial "$serial")
fi
cmd+=(print "$out" --dither)

echo "+ ${cmd[*]}"
"${cmd[@]}"
