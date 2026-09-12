//! Handing the notes field to `$EDITOR`: the temp file, the command, the round
//! trip. The terminal suspend lives in the main loop; the harness runs the
//! editor inline with a null stdin.

use std::path::PathBuf;

/// One editing session: what was written, and how to run the editor.
#[derive(Debug, Clone)]
pub struct EditorJob {
    pub tmp: PathBuf,
    pub command: Vec<String>,
}

/// `$EDITOR`, else `vim`, as the Python remtui resolves it.
pub fn resolve_editor(env: &dyn Fn(&str) -> Option<String>) -> String {
    env("EDITOR")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "vim".to_string())
}

/// Write `text` to a temp `.md` file and build the editor command.
pub fn prepare(text: &str, editor: &str) -> std::io::Result<EditorJob> {
    let file = tempfile::Builder::new()
        .prefix("remtui-")
        .suffix(".md")
        .tempfile()?;
    let (_, tmp) = file.keep()?;
    std::fs::write(&tmp, text)?;
    let mut command = shlex::split(editor)
        .unwrap_or_else(|| editor.split_whitespace().map(str::to_string).collect());
    if command.is_empty() {
        command.push("vim".into());
    }
    command.push(tmp.to_string_lossy().into_owned());
    Ok(EditorJob { tmp, command })
}

/// Run the editor to completion; `headless` gives it a null stdin.
pub fn run(job: &EditorJob, headless: bool) -> std::io::Result<()> {
    let (program, args) = job
        .command
        .split_first()
        .ok_or_else(|| std::io::Error::other("empty editor command"))?;
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if headless {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }
    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{program} exited with {status}"
        )))
    }
}

/// The edited text, or the error reading it.
pub fn result(job: &EditorJob) -> std::io::Result<String> {
    std::fs::read_to_string(&job.tmp)
}

pub fn cleanup(job: &EditorJob) {
    let _ = std::fs::remove_file(&job.tmp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_resolution_and_round_trip() {
        assert_eq!(resolve_editor(&|_| None), "vim");
        assert_eq!(
            resolve_editor(&|k| (k == "EDITOR").then(|| " code -w ".to_string())),
            "code -w"
        );
        let job = prepare("hello", "sh -c 'printf edited > \"$0\"'").unwrap();
        assert_eq!(job.command[0], "sh");
        run(&job, true).unwrap();
        assert_eq!(result(&job).unwrap(), "edited");
        cleanup(&job);
        assert!(!job.tmp.exists());
    }
}
