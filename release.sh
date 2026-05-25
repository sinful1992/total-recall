#!/usr/bin/env bash
# Usage: ./release.sh <version>   e.g.  ./release.sh 1.5.7
#
# Bumps the version in Cargo.toml and tauri.conf.json, commits, tags, and pushes.
# Refuses to run if there are uncommitted changes or if the version is already tagged.

set -euo pipefail

VERSION="${1:-}"
if [[ -z "$VERSION" ]]; then
  echo "Usage: $0 <version>   e.g.  $0 1.5.7" >&2
  exit 1
fi

# Strip leading 'v' if someone passes v1.5.7
VERSION="${VERSION#v}"

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "ERROR: version must be MAJOR.MINOR.PATCH (got '$VERSION')" >&2
  exit 1
fi

TAG="v$VERSION"

# Must be on main / no dirty working tree
if [[ -n "$(git status --porcelain)" ]]; then
  echo "ERROR: working tree has uncommitted changes — commit or stash first" >&2
  exit 1
fi

if git rev-parse "$TAG" &>/dev/null; then
  echo "ERROR: tag $TAG already exists" >&2
  exit 1
fi

CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$CURRENT_BRANCH" != "main" ]]; then
  echo "WARNING: you are on branch '$CURRENT_BRANCH', not main"
  read -r -p "Continue anyway? [y/N] " yn
  [[ "$yn" =~ ^[Yy]$ ]] || exit 1
fi

echo "Bumping version to $VERSION..."

# Cargo.toml — first 'version = "..."' line in [package]
sed -i "0,/^version = \"[^\"]*\"/s//version = \"$VERSION\"/" Cargo.toml

# tauri.conf.json
python3 - <<PYEOF
import json, sys
with open('tauri.conf.json') as f:
    d = json.load(f)
d['version'] = '$VERSION'
with open('tauri.conf.json', 'w') as f:
    json.dump(d, f, indent=2)
    f.write('\n')
PYEOF

# Verify both files now contain the right version before committing
CARGO_VER=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
TAURI_VER=$(python3 -c "import json; print(json.load(open('tauri.conf.json'))['version'])")
if [[ "$CARGO_VER" != "$VERSION" || "$TAURI_VER" != "$VERSION" ]]; then
  echo "ERROR: version mismatch after edit — Cargo=$CARGO_VER, Tauri=$TAURI_VER" >&2
  exit 1
fi

git add Cargo.toml tauri.conf.json
git commit -m "chore: bump version to $VERSION"
git tag "$TAG" -m "Release $TAG"
git push origin "$CURRENT_BRANCH"
git push origin "$TAG"

echo ""
echo "Released $TAG — CI is building now."
echo "https://github.com/sinful1992/total-recall/releases/tag/$TAG"
