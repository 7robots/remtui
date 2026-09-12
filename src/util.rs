//! Small helpers with no better home: paths, `which`, first lines.

use std::path::{Path, PathBuf};

/// `$HOME`, or `/` when unset. remtui is macOS software; there is always a home.
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// `~` and `~/x` expanded, anything else returned as written.
pub fn expand_tilde(text: &str) -> PathBuf {
    if text == "~" {
        return home_dir();
    }
    if let Some(rest) = text.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(text)
}

/// Is this an existing file the current user may execute?
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

/// `shutil.which`: a name with a slash is checked as given, otherwise `PATH`
/// is searched.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.contains('/') {
        let path = expand_tilde(name);
        return is_executable(&path).then_some(path);
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

/// The first non-blank line, trimmed, or "".
pub fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Truncate to `max` characters, ending in `…` when anything was cut.
pub fn ellipsize(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expands_only_at_the_front() {
        assert_eq!(expand_tilde("~/x"), home_dir().join("x"));
        assert_eq!(expand_tilde("/a/~"), PathBuf::from("/a/~"));
        assert_eq!(expand_tilde("~"), home_dir());
    }

    #[test]
    fn which_finds_sh_and_not_nonsense() {
        assert!(which("sh").is_some());
        assert!(which("no-such-binary-xyz").is_none());
        assert!(which("/bin/sh").is_some());
    }

    #[test]
    fn first_line_skips_blanks() {
        assert_eq!(first_line("\n  \n  error here \nmore"), "error here");
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn ellipsize_cuts_with_a_mark() {
        assert_eq!(ellipsize("abc", 3), "abc");
        assert_eq!(ellipsize("abcd", 3), "ab…");
    }
}
