//! File > Package Game…: cook the open place and assemble a playable folder
//! (scripts/package-place.sh). The job runs off the UI thread; its output
//! lines go to the Output panel and the launcher path is reported at the end.
//!
//! This is the development "run this build" folder. Immutable, versioned
//! distribution releases stay with scripts/release-bundle.sh.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

pub enum Msg {
    Line(String),
    Done(Result<PathBuf, String>),
}

pub struct Job {
    rx: Receiver<Msg>,
    /// `<place folder>/package/<id>/<id>`: what to run when it finishes.
    pub launcher: PathBuf,
}

/// Start packaging `place` (already saved to disk).
pub fn start(place: &Path, signed_in: bool) -> Result<Job, String> {
    if cfg!(windows) {
        return Err(
            "Package Game is not available on Windows yet: the packager is scripts/package-place.sh"
                .into(),
        );
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("scripts/package-place.sh");
    if !script.is_file() {
        return Err(format!(
            "{} is missing; Package Game needs the fork's source tree",
            script.display()
        ));
    }
    let stem = place
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("the place has no file name")?;
    let id = game_id(stem);
    let out = place.parent().ok_or("the place has no folder")?.join("package");
    let launcher = out.join(&id).join(&id);
    let job_launcher = launcher.clone();
    let (tx, rx) = channel();
    let place = place.to_path_buf();
    let root = root.to_path_buf();
    let id_arg = id.clone();
    let name_arg = stem.to_string();
    std::thread::Builder::new()
        .name("rustudio-package".into())
        .spawn(move || {
            let mut cmd = Command::new("bash");
            cmd.arg(&script)
                .arg("--place")
                .arg(&place)
                .arg("--id")
                .arg(&id_arg)
                .arg("--name")
                .arg(&name_arg)
                .arg("--out")
                .arg(&out)
                .arg("--allow-missing");
            if signed_in {
                cmd.arg("--signed-in");
            }
            cmd.current_dir(&root)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = match cmd.spawn() {
                Ok(child) => child,
                Err(e) => {
                    let _ = tx.send(Msg::Done(Err(format!("can't run bash {}: {e}", script.display()))));
                    return;
                }
            };
            let stderr = child.stderr.take().map(|err| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for line in BufReader::new(err).lines().map_while(Result::ok) {
                        let _ = tx.send(Msg::Line(line));
                    }
                })
            });
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    let _ = tx.send(Msg::Line(line));
                }
            }
            if let Some(t) = stderr {
                let _ = t.join();
            }
            let done = match child.wait() {
                Ok(status) if status.success() => Ok(launcher),
                Ok(status) => Err(format!("packaging failed ({status}); see Output above")),
                Err(e) => Err(format!("lost the packaging process: {e}")),
            };
            let _ = tx.send(Msg::Done(done));
        })
        .map_err(|e| format!("can't start the packaging thread: {e}"))?;
    Ok(Job { rx, launcher: job_launcher })
}

/// The game id and folder name from a place's stem: path-safe and not hidden.
fn game_id(stem: &str) -> String {
    let mut out: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.is_empty() {
        "game".into()
    } else {
        out
    }
}

/// Drain the job's output; `done` is set once when the script exits.
pub fn poll(job: &Job) -> (Vec<String>, Option<Result<PathBuf, String>>) {
    let mut lines = Vec::new();
    let mut done = None;
    while let Ok(msg) = job.rx.try_recv() {
        match msg {
            Msg::Line(line) => lines.push(line),
            Msg::Done(result) => {
                done = Some(result);
                break;
            }
        }
    }
    (lines, done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_ids_are_path_safe_and_not_hidden() {
        assert_eq!(game_id("My Game"), "My_Game");
        assert_eq!(game_id("..game"), "game");
        assert_eq!(game_id("a/b:c"), "a_b_c");
        assert_eq!(game_id(""), "game");
        assert_eq!(game_id("test-1.2"), "test-1.2");
    }
}
