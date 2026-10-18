//! Project-relative path normalization and glob matching.
//!
//! All workspace paths are project-relative. Before any permission check or
//! base lookup, a path is normalized: separators are unified, `.` segments are
//! dropped, `..` segments pop the previous segment, and anything that would
//! escape the root (or an absolute path) is refused. Matching is done on the
//! normalized form so `a/./b` and `a/../a/b` cannot smuggle access.

use crate::FsError;

/// Normalize a project-relative path, refusing escapes and absolute paths.
///
/// Returns the canonical `/`-separated relative path. An empty result is
/// refused (a workspace operation always names a file).
pub fn normalize(input: &str) -> Result<String, FsError> {
    if input.is_empty() {
        return Err(FsError::InvalidPath(input.to_string()));
    }
    let unified = input.replace('\\', "/");

    // Absolute paths and Windows drive/UNC roots are never project-relative.
    if unified.starts_with('/') || has_drive_prefix(&unified) {
        return Err(FsError::InvalidPath(input.to_string()));
    }

    let mut out: Vec<&str> = Vec::new();
    for segment in unified.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if out.pop().is_none() {
                    return Err(FsError::InvalidPath(input.to_string()));
                }
            }
            other => {
                if other.contains('\0') {
                    return Err(FsError::InvalidPath(input.to_string()));
                }
                out.push(other);
            }
        }
    }

    if out.is_empty() {
        return Err(FsError::InvalidPath(input.to_string()));
    }
    Ok(out.join("/"))
}

fn has_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

/// A `/`-separated glob pattern with `*`, `?`, and `**` segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    raw: String,
    segments: Vec<String>,
}

impl Pattern {
    pub fn parse(pattern: &str) -> Result<Pattern, FsError> {
        let raw = pattern.replace('\\', "/");
        if raw.is_empty() || raw.starts_with('/') || has_drive_prefix(&raw) {
            return Err(FsError::InvalidPath(pattern.to_string()));
        }
        let mut segments: Vec<String> = Vec::new();
        for segment in raw.split('/') {
            match segment {
                "" | "." => {}
                ".." => return Err(FsError::InvalidPath(pattern.to_string())),
                other => segments.push(other.to_string()),
            }
        }
        if segments.is_empty() {
            return Err(FsError::InvalidPath(pattern.to_string()));
        }
        Ok(Pattern { raw, segments })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Does this pattern match the (already normalized) `path`?
    pub fn matches(&self, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').collect();
        let pattern: Vec<&str> = self.segments.iter().map(String::as_str).collect();
        matches_segments(&pattern, &parts)
    }

    /// Does every path matched by `other` also match `self`?
    ///
    /// This is deliberately conservative: when containment cannot be proven it
    /// returns `false`, so a derived workspace is refused rather than widened.
    pub fn covers(&self, other: &Pattern) -> bool {
        let a: Vec<&str> = self.segments.iter().map(String::as_str).collect();
        let b: Vec<&str> = other.segments.iter().map(String::as_str).collect();
        covers_segments(&a, &b)
    }
}

fn matches_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((seg, rest)) => {
            if *seg == "**" {
                for skip in 0..=path.len() {
                    if matches_segments(rest, &path[skip..]) {
                        return true;
                    }
                }
                return false;
            }
            match path.split_first() {
                Some((first, tail)) if match_segment(seg, first) => matches_segments(rest, tail),
                _ => false,
            }
        }
    }
}

/// Match a single path segment against a single non-`**` pattern segment.
fn match_segment(pattern: impl AsRef<str>, segment: impl AsRef<str>) -> bool {
    let p: Vec<char> = pattern.as_ref().chars().collect();
    let s: Vec<char> = segment.as_ref().chars().collect();
    match_from(&p, &s)
}

fn match_from(p: &[char], s: &[char]) -> bool {
    match p.split_first() {
        None => s.is_empty(),
        Some(('*', rest)) => {
            for skip in 0..=s.len() {
                if match_from(rest, &s[skip..]) {
                    return true;
                }
            }
            false
        }
        Some(('?', rest)) => match s.split_first() {
            Some((_, tail)) => match_from(rest, tail),
            None => false,
        },
        Some((c, rest)) => match s.split_first() {
            Some((sc, tail)) if sc == c => match_from(rest, tail),
            _ => false,
        },
    }
}

fn covers_segments(a: &[&str], b: &[&str]) -> bool {
    let Some((first, rest)) = a.split_first() else {
        return b.is_empty();
    };
    if *first == "**" {
        for skip in 0..=b.len() {
            if covers_segments(rest, &b[skip..]) {
                return true;
            }
        }
        return false;
    }
    let Some((b_first, b_tail)) = b.split_first() else {
        return false;
    };
    // `b`'s `**` can match more than one segment, which only `a`'s own `**`
    // (handled above) could cover.
    if *b_first == "**" {
        return false;
    }
    if !segment_covers(first, b_first) {
        return false;
    }
    covers_segments(rest, b_tail)
}

/// Whether single-segment pattern `a` matches every path that pattern `b` does.
fn segment_covers(a: &str, b: &str) -> bool {
    if a == b || a == "*" {
        return true;
    }
    // A wildcard pattern can cover a concrete literal it matches, but a
    // wildcard `b` can only be covered by an equal pattern or `*`.
    if !contains_wildcard(b) {
        return match_segment(a, b);
    }
    false
}

fn contains_wildcard(segment: &str) -> bool {
    segment.contains('*') || segment.contains('?')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dots_and_separators() {
        assert_eq!(normalize("a/./b").unwrap(), "a/b");
        assert_eq!(normalize("a/../a/b").unwrap(), "a/b");
        assert_eq!(
            normalize("src\\frontend\\app.ax").unwrap(),
            "src/frontend/app.ax"
        );
    }

    #[test]
    fn refuses_escapes_and_absolute_paths() {
        assert!(normalize("../secret").is_err());
        assert!(normalize("a/../../b").is_err());
        assert!(normalize("/etc/passwd").is_err());
        assert!(normalize("C:/Windows").is_err());
    }

    #[test]
    fn glob_matching() {
        let p = Pattern::parse("src/frontend/**").unwrap();
        assert!(p.matches("src/frontend/app.ax"));
        assert!(p.matches("src/frontend/nested/deep.ax"));
        assert!(!p.matches("src/server/app.ax"));

        let single = Pattern::parse("src/*.ax").unwrap();
        assert!(single.matches("src/a.ax"));
        assert!(!single.matches("src/nested/a.ax"));
    }

    #[test]
    fn glob_containment_is_conservative() {
        let wide = Pattern::parse("src/**").unwrap();
        let narrow = Pattern::parse("src/frontend/**").unwrap();
        assert!(wide.covers(&narrow));
        assert!(!narrow.covers(&wide));

        let any = Pattern::parse("**").unwrap();
        assert!(any.covers(&wide));

        let star_ax = Pattern::parse("src/*.ax").unwrap();
        let literal = Pattern::parse("src/app.ax").unwrap();
        assert!(star_ax.covers(&literal));
        assert!(!literal.covers(&star_ax));
    }
}
