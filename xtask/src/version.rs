//! `cargo xtask version [set X.Y.Z[-pre]]`: the workspace version, the single source of truth.
//!
//! The version lives in `[workspace.package] version` in the root `Cargo.toml`; every crate
//! inherits it with `version.workspace = true`, and the packaging scripts read it from here. The
//! internal crates are pinned in `[workspace.dependencies]`, and `set` moves those pins with it.

use std::path::Path;

/// The `version = "…"` value inside `[workspace.package]`.
pub fn read(manifest: &str) -> Result<String, String> {
    let (_, line) = find_line(manifest)?;
    let v = line.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string()).unwrap_or_default();
    if v.is_empty() {
        return Err("empty [workspace.package] version".into());
    }
    Ok(v)
}

/// `manifest` with the workspace version replaced; everything else is byte-for-byte unchanged.
///
/// The internal crates are pinned in `[workspace.dependencies]` with an explicit `version`, and a
/// pre-release does not satisfy the previous pin (`0.3.0-rc.1` is below `^0.3.0`), so cargo cannot
/// resolve the workspace with only `[workspace.package]` moved. Those pins travel with it.
pub fn replace(manifest: &str, new: &str) -> Result<String, String> {
    validate(new)?;
    let (idx, _) = find_line(manifest)?;
    let old = read(manifest)?;
    let mut out = String::with_capacity(manifest.len() + 8);
    let mut in_deps = false;
    for (i, line) in manifest.split_inclusive('\n').enumerate() {
        if i == idx {
            let eol = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(&format!("version = \"{new}\"{eol}"));
            continue;
        }
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[workspace.dependencies]";
        } else if in_deps && t.contains("path = ") {
            out.push_str(&repin(line, new));
            continue;
        }
        out.push_str(line);
    }
    Ok(out)
}

/// One `[workspace.dependencies]` line with its `version = "…"` set to `new`, path untouched.
///
/// Upstream never has to move these pins — a plain release satisfies the pin of the release before
/// it — so a synced upstream release leaves them behind the workspace version. Ours is a
/// pre-release, which no older pin accepts, so whatever the pin said, it follows the new version.
fn repin(line: &str, new: &str) -> String {
    let Some(start) = line.find("version = \"") else {
        return line.to_string();
    };
    let tail = &line[start + "version = \"".len()..];
    let Some(end) = tail.find('"') else {
        return line.to_string();
    };
    format!("{}version = \"{new}\"{}", &line[..start], &tail[end + 1..])
}

/// Semver without build metadata: `MAJOR.MINOR.PATCH` plus an optional `-pre.release` tag.
pub fn validate(v: &str) -> Result<(), String> {
    let bad = || Err(format!("`{v}` is not a version like 1.2.3 or 1.2.3-rc.1"));
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let nums: Vec<&str> = core.split('.').collect();
    if nums.len() != 3 || nums.iter().any(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) || (n.len() > 1 && n.starts_with('0'))) {
        return bad();
    }
    if let Some(p) = pre
        && (p.is_empty() || p.split('.').any(|id| id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')))
    {
        return bad();
    }
    Ok(())
}

/// Index and text of the `version` line in `[workspace.package]`.
fn find_line(manifest: &str) -> Result<(usize, &str), String> {
    let mut in_section = false;
    for (i, line) in manifest.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_section = t == "[workspace.package]";
            continue;
        }
        if in_section
            && let Some(rest) = t.strip_prefix("version")
            && rest.trim_start().starts_with('=')
        {
            return Ok((i, line));
        }
    }
    Err("no `version = \"…\"` in [workspace.package] of the root Cargo.toml".into())
}

pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    match args {
        [] => {
            println!("{}", read(&text)?);
            Ok(())
        }
        ["set", new] => {
            let old = read(&text)?;
            let updated = replace(&text, new)?;
            // Write atomically: a half-written root manifest breaks every build in the workspace.
            let tmp = root.join("target").join("Cargo.toml.xtask-version");
            std::fs::create_dir_all(root.join("target")).map_err(|e| format!("create target/: {e}"))?;
            std::fs::write(&tmp, updated).map_err(|e| format!("write {}: {e}", tmp.display()))?;
            std::fs::rename(&tmp, &path).map_err(|e| format!("replace {}: {e}", path.display()))?;
            // Refresh the workspace members' entries in Cargo.lock (no dependency upgrades).
            let cargo_update = |extra: &[&str]| -> Result<(), String> {
                let st = std::process::Command::new(env!("CARGO"))
                    .current_dir(root)
                    .args(["update", "--workspace"])
                    .args(extra)
                    .status()
                    .map_err(|e| format!("cargo update: {e}"))?;
                if st.success() { Ok(()) } else { Err(format!("cargo update --workspace {} failed", extra.join(" "))) }
            };
            if cargo_update(&["--offline"]).is_err() {
                cargo_update(&[])?;
            }
            println!("version: {old} -> {new}");
            Ok(())
        }
        _ => Err("usage: cargo xtask version [set X.Y.Z[-pre]]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace.dependencies]\nfoo = { version = \"1\" }\n";

    #[test]
    fn reads_the_workspace_version() {
        assert_eq!(read(MANIFEST).unwrap(), "0.1.0");
        assert!(read("[package]\nversion = \"1.0.0\"\n").is_err());
    }

    #[test]
    fn replaces_only_the_workspace_version() {
        let out = replace(MANIFEST, "1.2.3-rc.1").unwrap();
        assert_eq!(read(&out).unwrap(), "1.2.3-rc.1");
        assert_eq!(out, MANIFEST.replace("version = \"0.1.0\"", "version = \"1.2.3-rc.1\""));
        assert!(out.contains("foo = { version = \"1\" }"));
    }

    /// A pre-release is below the previous pin, so `^0.1.0` does not match `0.1.0-rc.1` and cargo
    /// fails to resolve the workspace unless the internal pins move with `[workspace.package]`.
    #[test]
    fn moves_the_internal_pins_with_the_version() {
        let m = "[workspace.package]\nversion = \"0.1.0\"\n\n[workspace.dependencies]\nin = { path = \"crates/in\", version = \"0.1.0\" }\nout = { version = \"0.1.0\" }\n\n[dependencies]\nother = { path = \"x\", version = \"0.1.0\" }\n";
        let want = "[workspace.package]\nversion = \"0.1.0-rc.1\"\n\n[workspace.dependencies]\nin = { path = \"crates/in\", version = \"0.1.0-rc.1\" }\nout = { version = \"0.1.0\" }\n\n[dependencies]\nother = { path = \"x\", version = \"0.1.0\" }\n";
        assert_eq!(replace(m, "0.1.0-rc.1").unwrap(), want);
    }

    /// The state a synced upstream release leaves behind: upstream moves only `[workspace.package]`,
    /// so the pins sit one version back — and a pre-release of the new version satisfies none of them.
    #[test]
    fn moves_internal_pins_that_lag_the_workspace() {
        let m = "[workspace.package]\nversion = \"0.5.1\"\n\n[workspace.dependencies]\nin = { path = \"crates/in\", version = \"0.5.0\" }\nout = { version = \"0.5.0\" }\n";
        let want = "[workspace.package]\nversion = \"0.5.1-cn.1\"\n\n[workspace.dependencies]\nin = { path = \"crates/in\", version = \"0.5.1-cn.1\" }\nout = { version = \"0.5.0\" }\n";
        assert_eq!(replace(m, "0.5.1-cn.1").unwrap(), want);
    }

    #[test]
    fn keeps_crlf_line_endings() {
        let crlf = MANIFEST.replace('\n', "\r\n");
        let out = replace(&crlf, "2.0.0").unwrap();
        assert_eq!(out, crlf.replace("\"0.1.0\"", "\"2.0.0\""));
    }

    #[test]
    fn validates_versions() {
        for ok in ["0.1.0", "1.20.300", "1.0.0-rc.1", "1.0.0-alpha", "1.0.0-x-y.2"] {
            assert!(validate(ok).is_ok(), "{ok}");
        }
        for bad in ["1.0", "1.0.0.0", "v1.0.0", "01.0.0", "1.0.0-", "1.0.0-a..b", "1.0.0+meta", "1.a.0", ""] {
            assert!(validate(bad).is_err(), "{bad}");
        }
        assert!(replace(MANIFEST, "nope").is_err());
    }

    #[test]
    fn the_real_manifest_has_a_valid_version() {
        let text = std::fs::read_to_string(crate::gates::root().join("Cargo.toml")).unwrap();
        validate(&read(&text).unwrap()).unwrap();
    }

    /// The internal pins are exactly what a `-rc.1` bump trips over, so they never drift from the
    /// workspace version.
    #[test]
    fn the_real_manifest_pins_internal_crates_at_its_version() {
        let text = std::fs::read_to_string(crate::gates::root().join("Cargo.toml")).unwrap();
        let version = read(&text).unwrap();
        let mut in_deps = false;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                in_deps = t == "[workspace.dependencies]";
                continue;
            }
            if in_deps && t.contains("path = ") {
                let pin = t.split("version = \"").nth(1).and_then(|rest| rest.split('"').next()).unwrap_or_default();
                assert!(pin.is_empty() || pin == version, "{t}: pinned at {pin}, workspace is {version}");
            }
        }
    }
}
