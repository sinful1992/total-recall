#!/usr/bin/env bash
# PreToolUse hook — blocks git push of a v* tag if versions don't match.
# Claude Code passes the tool input as JSON on stdin.

CMD=$(python3 -c "import sys,json; print(json.load(sys.stdin).get('command',''))")

# Only care about git push with a version tag
echo "$CMD" | grep -qE 'git push' || exit 0
TAG=$(echo "$CMD" | grep -oE '\bv[0-9]+\.[0-9]+\.[0-9]+\b' | head -1)
[[ -z "$TAG" ]] && exit 0
TAG="${TAG#v}"

CARGO=$(python3 -c "import re; print(re.search(r'^version = \"(.+?)\"', open('Cargo.toml').read(), re.M).group(1))")
TAURI=$(python3 -c "import json; print(json.load(open('tauri.conf.json'))['version'])")

if [[ "$CARGO" != "$TAG" || "$TAURI" != "$TAG" ]]; then
    echo "Version mismatch — cannot push tag v$TAG"
    echo "  Cargo.toml:      $CARGO"
    echo "  tauri.conf.json: $TAURI"
    echo "  Bump both to $TAG and commit first."
    exit 2
fi
