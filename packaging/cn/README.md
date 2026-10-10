# CN release automation

Downstream-only scripts (`cn` branch; they never exist on `zh-fallback-all`, the head branch of
upstream PR #687). They turn "upstream moved" into a published, signed release without a person
remembering the fifteen steps in between.

```
upstream main ──▶ sync-upstream.sh ──▶ cn (merged, gates green, new -cn.N version)
                                        │
                                        └──▶ publish-signed.sh ──▶ GitHub Release, published
```

## `sync-upstream.sh`

`packaging/cn/sync-upstream.sh [--dry-run]`

Fast-forwards `main` on the fork, merges upstream `main` into a `cn-sync` scratch branch and bumps
the version to `<upstream>-cn.<N>`, where N is one past the highest tag already released. Prints
`NO-CHANGE <sha>`, `SYNCED <version>` or `DRY-RUN <version>`.

It stops instead of guessing. It refuses to run on a dirty tree or a `cn` that differs from the
pushed one; it aborts the merge untouched unless the conflict is *only* `Cargo.toml` / `Cargo.lock`,
and even then only after proving `cn` edits nothing in that manifest besides version strings — the
one conflict every sync hits, because upstream moves `0.5.0 → 0.5.1` while our line reads
`0.5.0-cn.3`. It also refuses a merge that put `macos-15-xlarge` back into `release.yml` (this
account cannot start that runner, so the macOS job would silently never run), and nothing is pushed
unless `cargo metadata --locked` and `cargo xtask ci` pass.

## `publish-signed.sh`

`packaging/cn/publish-signed.sh --notes FILE [--version X.Y.Z-cn.N] [--check]`

Pushes `release`, waits for `release.yml`, then rebuilds macOS **on this Mac** with the Developer ID
identity and Apple notarization, replaces the two macOS artifacts on the draft release, repairs
`SHA256SUMS.txt` (exactly two lines may change), publishes the draft and reads the asset digests
back to prove the server holds what it signed.

CI cannot do this part: the fork's `release` environment holds no certificates, so its macOS
artifacts come out ad-hoc signed (`docs/releasing.md`). The private key stays in this keychain.
Nothing secret is in these scripts — an identity name, a `notarytool` keychain profile name and a
craft-fonts checkout path are machine-local settings, all overridable through the environment
(`MACOS_SIGN_IDENTITY`, `NOTARY_KEYCHAIN_PROFILE`, `CRAFT_FONTS_DIR`).

`--check` only proves this machine can sign and notarize, and costs a few seconds.

The release notes are a required file rather than a template, because saying what a release means
for users is a judgement call. A profile name proves nothing on its own: the script authenticates
with `notarytool history` before it builds anything.

## The daily job

An automation runs `sync-upstream.sh`, and only on `SYNCED` writes notes for what upstream brought
and runs `publish-signed.sh`. It is a local (not cloud) task: signing and notarization need this
Mac's keychain, so the run only happens while the machine is awake and Qoder is running. Both
scripts are safe to run by hand at any time, and both are idempotent enough to re-run after a stop.
