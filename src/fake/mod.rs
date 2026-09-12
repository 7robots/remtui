//! Stand-ins for `remctl` and `bearcli`, built as their own binaries.
//!
//! They implement the subset remtui drives with the real tools' contracts and
//! the same state files as the Python fakes, so `remtui --demo` needs no
//! Reminders and no Python, and the Python test suite can run against them.

pub mod bearcli;
pub mod remctl;

use std::path::Path;

/// Write `text` to `path` through a sibling temp file and a rename, so a
/// concurrent reader never sees a half-written file.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!("{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// A minimal argv parser in argparse's style: `--flag`, `--opt value`,
/// `--opt=value`, short aliases, and `--` ending option parsing.
pub struct Args {
    pub positional: Vec<String>,
    pub options: std::collections::BTreeMap<String, String>,
    pub flags: std::collections::BTreeSet<String>,
}

impl Args {
    /// `takes_value` names options that consume the next token; `aliases` maps
    /// short forms to long names (`-l` → `list`).
    pub fn parse(
        argv: &[String],
        takes_value: &[&str],
        aliases: &[(&str, &str)],
    ) -> Result<Args, String> {
        let mut args = Args {
            positional: Vec::new(),
            options: Default::default(),
            flags: Default::default(),
        };
        let mut i = 0;
        let mut only_positional = false;
        while i < argv.len() {
            let tok = &argv[i];
            if only_positional || !tok.starts_with('-') || tok == "-" {
                args.positional.push(tok.clone());
                i += 1;
                continue;
            }
            if tok == "--" {
                only_positional = true;
                i += 1;
                continue;
            }
            let (raw_name, inline) = match tok.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
                _ => (tok.clone(), None),
            };
            let name = if let Some(long) = raw_name.strip_prefix("--") {
                long.to_string()
            } else {
                let short = raw_name.trim_start_matches('-');
                aliases
                    .iter()
                    .find(|(s, _)| *s == short)
                    .map(|(_, l)| l.to_string())
                    .ok_or_else(|| format!("unrecognized arguments: {tok}"))?
            };
            if takes_value.contains(&name.as_str()) {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        argv.get(i)
                            .cloned()
                            .ok_or_else(|| format!("argument --{name}: expected one argument"))?
                    }
                };
                args.options.insert(name, value);
            } else {
                if inline.is_some() {
                    return Err(format!("argument --{name}: ignored explicit argument"));
                }
                args.flags.insert(name);
            }
            i += 1;
        }
        Ok(args)
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    pub fn opt(&self, name: &str) -> Option<&str> {
        self.options.get(name).map(String::as_str)
    }
}
