#!/usr/bin/env bash
# packaging/cn/sync-upstream.sh — fold upstream `storytold/pdfcraft` main into our `cn` branch.
#
# Downstream plumbing only. It never pushes to the `origin` remote (that is upstream's repository)
# and never touches `zh-fallback-all`, the head branch of upstream PR #687: CN-only work must not
# reach upstream review (AGENTS.md §1, docs/releasing.md).
#
# One terminal line tells the caller what happened:
#   NO-CHANGE <sha>    upstream main is already inside cn
#   SYNCED <version>   merged, verified, bumped and pushed; release it with publish-signed.sh
# Anything else exits non-zero with nothing pushed, leaving the merge on the local `cn-sync`
# branch for a human to look at.
#
# --dry-run: merge and verify, then stop before pushing.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FORK_REMOTE=${FORK_REMOTE:-me}
UPSTREAM_REMOTE=${UPSTREAM_REMOTE:-origin}
SCRATCH=cn-sync
DRY_RUN=0
[ "${1:-}" = "--dry-run" ] && DRY_RUN=1

die() { printf 'sync: %s\n' "$*" >&2; exit 1; }

# The version we ship is upstream's plus which CN build of it this is.
base_of() {
  sed -E 's/^[^"]*"([^"-]*).*/\1/' < <(git show "$1:Cargo.toml" | awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { print; exit }
  ')
}
next_cn_number() {
  # Highest `v<base>-cn.N` tag on the fork, so no run ever reuses a released version.
  local max=0 n
  while read -r _ tag; do
    n="${tag##*"-cn."}"
    if printf '%s' "$n" | grep -Eq '^[0-9]+$' && [ "$n" -gt "$max" ]; then max="$n"; fi
  done < <(git ls-remote --tags "$FORK_REMOTE" "refs/tags/v$1-cn.*")
  echo $((max + 1))
}
blank_versions() { sed -E 's/version *= *"[^"]*"/version = ""/g' "$1"; }

git fetch --quiet "$UPSTREAM_REMOTE" main
git fetch --quiet "$FORK_REMOTE" "cn" "main"
UP="$(git rev-parse "$UPSTREAM_REMOTE/main")"
CN="$(git rev-parse "$FORK_REMOTE/cn")"

if git merge-base --is-ancestor "$UP" "$CN"; then
  echo "NO-CHANGE $UP"
  exit 0
fi

# This runs unattended: a half-finished local edit must not reach a release.
[ "$(git rev-parse cn)" = "$CN" ] || die "local cn is not me/cn; finish or stash that work first"
[ -z "$(git status --porcelain)" ] || die "working tree is not clean"

# The auto-resolved version conflict below takes upstream's whole `Cargo.toml`, which is only
# safe while cn edits nothing in it besides version strings.
if ! diff -q <(git show "$UP:Cargo.toml" | blank_versions /dev/stdin) \
             <(git show "$CN:Cargo.toml" | blank_versions /dev/stdin) >/dev/null; then
  die "cn carries its own Cargo.toml edits; merge by hand"
fi

# Keep the mirror branch fast-forwarded too; nothing of ours is ever committed there.
if [ "$DRY_RUN" = 0 ] && git merge-base --is-ancestor "$FORK_REMOTE/main" "$UP"; then
  git push --quiet "$FORK_REMOTE" "$UP:refs/heads/main" \
    || echo "sync: could not fast-forward $FORK_REMOTE/main (continuing)" >&2
fi

git branch -f "$SCRATCH" "$CN"
git checkout --quiet "$SCRATCH"

if ! git merge --no-edit "$UP"; then
  # Only the version line is safe to settle alone: upstream moved 0.5.0 -> 0.5.1, ours reads
  # 0.5.0-cn.3. Take upstream's files — cn edits nothing in them besides versions, proven above —
  # and let `xtask version set` put the CN suffix back on and refresh the lock. Any other
  # conflict needs a human. (git lists the paths alphabetically, so test them one by one rather
  # than matching the whole string.)
  unresolved=""
  for f in $(git diff --name-only --diff-filter=U); do
    case " Cargo.toml Cargo.lock " in
      *" $f "*) ;;
      *) unresolved="$unresolved $f" ;;
    esac
  done
  if [ -n "$unresolved" ]; then
    git merge --abort
    die "merge conflicts in:$unresolved"
  fi
  for f in $(git diff --name-only --diff-filter=U); do
    # Write upstream's content straight out of the commit it came from rather than trusting the
    # conflicted index: `checkout --theirs` is state-dependent enough that a rehearsal of this
    # exact merge failed on it once, and the release must not depend on which form the index is in.
    git show "$UP:$f" > "$f"
    git add "$f"
  done
  git commit --no-edit --quiet
fi

BASE="$(base_of "$UP")"
[ -n "$BASE" ] || die "cannot read upstream's version from $UP"
NEW="$BASE-cn.$(next_cn_number "$BASE")"
if [ "$(cargo xtask version)" != "$NEW" ]; then
  cargo xtask version set "$NEW"
  git add Cargo.toml Cargo.lock
  git commit --quiet -m "Release: $NEW"
fi

# Upstream picks the billed larger macOS runner; this account cannot start it, so the macOS job
# would never run and the release would ship without it (docs/releasing.md).
grep -q "runs-on: macos-15-xlarge" .github/workflows/release.yml \
  && die "release.yml is back on macos-15-xlarge; keep the cn edit"

cargo metadata --locked --format-version 1 >/dev/null || die "Cargo.lock does not match the manifests"
cargo xtask ci || die "cargo xtask ci failed on $SCRATCH; nothing was pushed"

if [ "$DRY_RUN" = 1 ]; then
  echo "DRY-RUN $NEW (merged on $SCRATCH, nothing pushed)"
  exit 0
fi

git push --quiet "$FORK_REMOTE" "$SCRATCH:cn"
git branch -f cn "$SCRATCH"
git checkout --quiet cn
echo "SYNCED $NEW"
