#!/usr/bin/env bash
# packaging/cn/publish-signed.sh — ship the CN build the way this fork actually can.
#
# GitHub's runners hold no certificates for this fork, so CI's macOS artifacts come out ad-hoc
# signed and Gatekeeper rejects them (docs/releasing.md). This pushes `release`, waits for the
# pipeline, rebuilds macOS here with the Developer ID identity and Apple notarization, swaps those
# two artifacts for the signed ones, repairs `SHA256SUMS.txt`, then publishes the draft.
#
#   packaging/cn/publish-signed.sh --notes FILE [--version X.Y.Z-cn.N] [--check]
#
# The notes are a required argument on purpose: release notes are prose about what users get, and
# that is a person's (or an agent's) call, not a template's. `--check` only proves this machine can
# sign and notarize, then stops.
#
# Env: MACOS_SIGN_IDENTITY, NOTARY_KEYCHAIN_PROFILE, CRAFT_FONTS_DIR, CI_TIMEOUT, FORK_REMOTE,
# FORK_REPO. Refuses to publish unsigned work: if this machine cannot sign and notarize, the
# release stays a draft and the run fails with the reason.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FORK_REMOTE=${FORK_REMOTE:-me}
FORK_REPO=${FORK_REPO:-misswell/PdfCraft-CN}
# Machine-local settings, not secrets: a signing identity, a keychain profile name, a checkout
# path. Overridable so this works under another identity or on another Mac.
IDENTITY=${MACOS_SIGN_IDENTITY:-Developer ID Application: Guofeng Liu (U8U443D7ZL)}
NOTARY_PROFILE=${NOTARY_KEYCHAIN_PROFILE:-octoshrink-notary}
FONTS_DIR=${CRAFT_FONTS_DIR:-$ROOT/../craft-fonts}
CI_TIMEOUT=${CI_TIMEOUT:-5400}

VERSION=""
NOTES=""
CHECK=0
die() { printf 'publish: %s\n' "$*" >&2; exit 1; }
sha() { shasum -a 256 "$1" | awk '{print $1}'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --notes) NOTES="$2"; shift 2 ;;
    --check) CHECK=1; shift ;;
    *) die "unknown argument $1" ;;
  esac
done
if [ "$CHECK" = 0 ] && { [ -z "$NOTES" ] || [ ! -f "$NOTES" ]; }; then
  die "run this with --notes FILE (or --check to only test this machine)"
fi
[ -n "$VERSION" ] || VERSION="$(cargo xtask version)"
TAG="v$VERSION"
[ "$(cargo xtask version)" = "$VERSION" ] || die "--version $VERSION is not what Cargo.toml says"

# ---- can this machine really sign and notarize? ------------------------------------------------
security find-identity -v -p codesigning | grep -qF "$IDENTITY" \
  || die "no usable signing identity '$IDENTITY' in this keychain"
# A profile name proves nothing on its own; only a command that authenticates does.
xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" --output-format json >/dev/null \
  || die "notarytool profile '$NOTARY_PROFILE' does not authenticate here"
[ -d "$FONTS_DIR" ] || die "craft-fonts checkout missing at $FONTS_DIR"

# ---- run the pipeline ----------------------------------------------------------------------------
[ "$(git rev-parse HEAD)" = "$(git rev-parse cn)" ] || die "HEAD is not cn; publish the branch that was synced"
if [ "$CHECK" = 1 ]; then
  echo "CHECK-OK $IDENTITY / profile $NOTARY_PROFILE / $FONTS_DIR"
  exit 0
fi
BEFORE="$(gh api "repos/$FORK_REPO/actions/workflows/release.yml/runs?per_page=1" --jq '.workflow_runs[0].id')"
git branch -f release cn
git push --quiet "$FORK_REMOTE" release:release

echo "==> waiting for release.yml"
DEADLINE=$(( $(date +%s) + CI_TIMEOUT ))
RUN=""
while [ "${RUN:-}" = "" ] || [ "$RUN" = "$BEFORE" ]; do
  [ "$(date +%s)" -lt "$DEADLINE" ] || die "no new release.yml run appeared after $BEFORE"
  sleep 30
  RUN="$(gh api "repos/$FORK_REPO/actions/workflows/release.yml/runs?branch=release&per_page=1" \
    --jq '.workflow_runs[0].id // ""')"
done
echo "==> https://github.com/$FORK_REPO/actions/runs/$RUN"
while :; do
  STATUS="$(gh api "repos/$FORK_REPO/actions/runs/$RUN" --jq '.status')"
  if [ "$STATUS" = completed ]; then break; fi
  [ "$(date +%s)" -lt "$DEADLINE" ] || die "run $RUN still $STATUS after ${CI_TIMEOUT}s"
  sleep 60
done
CONCLUSION="$(gh api "repos/$FORK_REPO/actions/runs/$RUN" --jq '.conclusion')"
[ "$CONCLUSION" = success ] || die "run $RUN finished '$CONCLUSION'"

# ---- rebuild macOS here, signed and notarized ----------------------------------------------------
MACOS_SIGN_IDENTITY="$IDENTITY" NOTARY_KEYCHAIN_PROFILE="$NOTARY_PROFILE" \
CRAFT_FONTS_DIR="$FONTS_DIR" CRAFT_FONTS_REQUIRED=1 \
  packaging/macos/package.sh --arch universal

DMG="dist/release/pdfcraft-$VERSION-macos-universal.dmg"
CLI="dist/release/pdfcraft-cli-$VERSION-macos-universal.zip"
[ -f "$DMG" ] && [ -f "$CLI" ] || die "expected $DMG and $CLI"
spctl --assess --type install "$DMG" 2>/dev/null \
  || die "$DMG does not pass Gatekeeper"

# ---- swap the two macOS artifacts, then repair the checksums -------------------------------------
gh release upload "$TAG" "$DMG" "$CLI" --repo "$FORK_REPO" --clobber
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
gh release download "$TAG" --repo "$FORK_REPO" --pattern SHA256SUMS.txt --dir "$TMP"
cp "$TMP/SHA256SUMS.txt" "$TMP/original"
for file in "$DMG" "$CLI"; do
  awk -v want="$(basename "$file")" -v sum="$(sha "$file")" \
    '{ print ($2 == want ? sum "  " want : $0) }' "$TMP/SHA256SUMS.txt" > "$TMP/patched"
  mv "$TMP/patched" "$TMP/SHA256SUMS.txt"
done
# Only the two macOS lines may move; every other artifact is still exactly what CI uploaded.
CHANGED="$(diff "$TMP/original" "$TMP/SHA256SUMS.txt" | grep -c '^[<>]' || true)"
[ "$CHANGED" = 4 ] || die "SHA256SUMS.txt changed in $CHANGED places, expected 2"
gh release upload "$TAG" "$TMP/SHA256SUMS.txt" --repo "$FORK_REPO" --clobber

# ---- publish, and prove what the server now holds -------------------------------------------------
gh release edit "$TAG" --repo "$FORK_REPO" --draft=false --notes-file "$NOTES"
for file in "$DMG" "$CLI"; do
  REMOTE="$(gh release view "$TAG" --repo "$FORK_REPO" --json assets \
    --jq ".assets[] | select(.name==\"$(basename "$file")\") | .digest" | sed 's/^sha256://')"
  [ "$REMOTE" = "$(sha "$file")" ] || die "$(basename "$file") on the release is not the signed build"
done
echo "PUBLISHED https://github.com/$FORK_REPO/releases/tag/$TAG"
echo "  $ROOT/$DMG"
echo "  $ROOT/$CLI"
