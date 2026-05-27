// Boundary-input sanitisation for ``topo-extract-rust``.
//
// The extractor's stdin contract is ``{ "files": [...], ... }`` where
// each entry is a host-side path the extractor will ``std::fs::read``.
// Without sanitisation a malicious driver (or a compromised producer)
// can hand the extractor ``../../../etc/passwd`` and exfiltrate the
// contents through the extractor's downstream JSON emission. The Rust
// projection of the C++ ``topo::platform::sanitize_path`` API closes
// that loop.
//
// Stdlib-only on purpose; the principle this enforces is "validate
// every value at the system boundary, once" — downstream code in this
// crate can then assume a value coming through ``sanitize_path`` was
// resolved under a known-good root.

use std::path::{Component, Path, PathBuf};

/// Canonicalise ``input`` (joined under ``root`` if relative) and confirm
/// it stays under ``root``. Returns ``Err`` on any reject condition —
/// path-traversal segment, absolute path escaping root, symlink crossing
/// the root boundary, or empty input. The caller MUST treat ``Err`` as
/// a hard reject and never fall back to the verbatim string.
pub fn sanitize_path(input: &str, root: &Path) -> Result<PathBuf, String> {
    if input.is_empty() {
        return Err("empty path".to_string());
    }

    let raw = PathBuf::from(input);
    let root_canon = canonicalize_or_normalize(root);

    let joined: PathBuf = if raw.is_absolute() {
        raw
    } else {
        root_canon.join(&raw)
    };

    // Lexical reject of any residual ``..`` *before* touching the
    // filesystem. ``foo/../../etc/passwd`` normalises to
    // ``/etc/passwd``; the subpath check below would catch it, but the
    // lexical reject short-circuits the symlink-follow path entirely.
    let normalised = lexically_normalize(&joined);
    if normalised
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!("path escapes root via '..': {}", input));
    }

    let resolved = canonicalize_or_normalize(&normalised);
    if !is_subpath(&resolved, &root_canon) {
        return Err(format!(
            "path '{}' escapes root '{}'",
            resolved.display(),
            root_canon.display()
        ));
    }
    Ok(resolved)
}

/// True iff ``child`` is identical to or a descendant of ``root`` after
/// both are lexically normalised. Pure path algebra — no filesystem
/// access. Use this when both sides are already canonicalised.
pub fn is_subpath(child: &Path, root: &Path) -> bool {
    let c = lexically_normalize(child);
    let r = lexically_normalize(root);
    let mut ci = c.components();
    for rc in r.components() {
        match ci.next() {
            Some(cc) if cc == rc => continue,
            _ => return false,
        }
    }
    true
}

fn canonicalize_or_normalize(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| lexically_normalize(p))
}

fn lexically_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // Pop if we can — but only when the previous component
                // is a real Normal segment. RootDir / Prefix / a leading
                // ParentDir all stay as-is so the caller's subpath check
                // can correctly reject them.
                let popped = out.components().last().map(|c| match c {
                    Component::Normal(_) => true,
                    _ => false,
                });
                if popped == Some(true) {
                    out.pop();
                } else {
                    out.push(comp.as_os_str());
                }
            }
            _ => out.push(comp.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile_lite::TempDir;

    // Tiny inline scratch-dir helper to avoid adding a tempfile dep
    // just for tests. Uses the crate's ``CARGO_TARGET_TMPDIR`` if set
    // (cargo's documented test temp dir), else the system temp dir.
    mod tempfile_lite {
        use std::path::PathBuf;

        pub struct TempDir(pub PathBuf);
        impl TempDir {
            pub fn new(tag: &str) -> Self {
                let base = std::env::var_os("CARGO_TARGET_TMPDIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir);
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                let dir = base.join(format!("topo-rust-safety-{}-{}", tag, nonce));
                std::fs::create_dir_all(&dir).unwrap();
                TempDir(dir)
            }
        }
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn accepts_relative_subpath() {
        let tmp = TempDir::new("ok");
        fs::create_dir_all(tmp.0.join("sub")).unwrap();
        fs::write(tmp.0.join("sub/file.rs"), "fn main() {}").unwrap();
        let resolved = sanitize_path("sub/file.rs", &tmp.0).unwrap();
        assert!(resolved.starts_with(fs::canonicalize(&tmp.0).unwrap()));
    }

    #[test]
    fn rejects_parent_ref_attack() {
        let tmp = TempDir::new("parent");
        assert!(sanitize_path("../../etc/passwd", &tmp.0).is_err());
        assert!(sanitize_path("foo/../../etc/passwd", &tmp.0).is_err());
    }

    #[test]
    fn rejects_absolute_outside_root() {
        let tmp = TempDir::new("abs");
        assert!(sanitize_path("/etc/passwd", &tmp.0).is_err());
    }

    #[test]
    fn rejects_empty() {
        let tmp = TempDir::new("empty");
        assert!(sanitize_path("", &tmp.0).is_err());
    }

    #[test]
    fn is_subpath_basic() {
        assert!(is_subpath(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_subpath(Path::new("/a/b"), Path::new("/a/b")));
        assert!(!is_subpath(Path::new("/a/b2"), Path::new("/a/b")));
        assert!(!is_subpath(Path::new("/a"), Path::new("/a/b")));
    }
}
