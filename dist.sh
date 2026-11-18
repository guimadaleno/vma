#!/usr/bin/env bash
# Builds the Windows executable into dist/, signs it when a certificate is at
# hand, and holds it to its budget. It is the whole of what ships.
#
#   ./dist.sh                                           # dist/vma.exe
#   VMA_SIGNTOOL='signtool.exe sign /fd sha256 … $f' ./dist.sh
#
# Runs from Linux, WSL, macOS or Git Bash on Windows: nothing below is GNU-only.
set -euo pipefail

TARGET="${VMA_TARGET:-x86_64-pc-windows-gnu}"
EXE="dist/vma.exe"
# The claim is a tool quick to download and to start for every action. The
# budget is a test, not an aspiration: 1 MB leaves room for the MCP server and
# the agent inside the machine, which the same file carries.
BUDGET=$((1024 * 1024))

cargo build --release --target "$TARGET"
mkdir -p dist
cp "target/$TARGET/release/vma.exe" "$EXE"

# Signing, when a certificate is at hand. VMA_SIGNTOOL holds a whole command
# with $f where the file goes. It runs before the size check because the
# budget is a claim about what ships, and what ships carries the signature.
if [ -n "${VMA_SIGNTOOL:-}" ]; then
  target="$EXE"
  # signtool.exe is a Windows program however it is driven, and cannot read a
  # WSL path. Git Bash on a Windows runner has no wslpath and needs none.
  if command -v wslpath >/dev/null 2>&1; then
    target=$(wslpath -w "$target")
  fi
  echo "signing: $target"
  eval "${VMA_SIGNTOOL//\$f/\"$target\"}"
fi

# `wc -c` rather than `stat`, whose flags differ between GNU and BSD.
size=$(wc -c < "$EXE" | tr -d ' ')
echo "vma.exe: $((size / 1024)) KB"
if [ "$size" -gt "$BUDGET" ]; then
  echo "FAIL: binary exceeds the $((BUDGET / 1024)) KB budget" >&2
  exit 1
fi
echo "ready: $EXE"
