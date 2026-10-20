//! A recorder that is a program of its own (ffmpeg, wf-recorder, gst-launch):
//! started, watched, and asked to stop in the way each one finishes its file.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::Session;

/// How a recorder is told to finish its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    /// Type `q` to its input (ffmpeg).
    Quit,
    /// Send SIGINT (wf-recorder, and gst-launch with `-e`, which then ends the stream).
    Interrupt,
}

/// A program that fails within this long of starting never recorded anything:
/// a bad area, a missing encoder.
const STARTUP: Duration = Duration::from_millis(600);
/// How long it gets to finish the file once asked.
const FINISH: Duration = Duration::from_secs(20);
/// The end of its messages is kept, for the error if it fails.
const LOG_BYTES: usize = 4096;

pub struct Recorder {
    program: String,
    child: Arc<Mutex<Child>>,
    stdin: Option<ChildStdin>,
    how: Stop,
    path: PathBuf,
    log: Arc<Mutex<String>>,
    /// Set when we ask it to stop, so its ending is expected.
    stopping: Arc<AtomicBool>,
    /// Set if it ended before we asked.
    died: Arc<AtomicBool>,
}

/// Starts `command` and checks it stays up. `ended` is told if it later exits by itself.
pub fn spawn(
    program: &str,
    mut command: Command,
    how: Stop,
    path: PathBuf,
    ended: Sender<()>,
) -> Result<Recorder, String> {
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("{program}: {e}"))?;
    let stdin = child.stdin.take();
    let log = Arc::new(Mutex::new(String::new()));
    if let Some(mut stderr) = child.stderr.take() {
        let log = log.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = stderr.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut log = log.lock().unwrap();
                log.push_str(&String::from_utf8_lossy(&buf[..n]));
                if log.len() > LOG_BYTES {
                    let mut cut = log.len() - LOG_BYTES;
                    while !log.is_char_boundary(cut) {
                        cut += 1;
                    }
                    log.drain(..cut);
                }
            }
        });
    }

    let started = Instant::now();
    while started.elapsed() < STARTUP {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            // Let the message reader catch up with what it printed last.
            std::thread::sleep(Duration::from_millis(50));
            return Err(format!("{program} stopped at once ({status}): {}", tail(&log)));
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let child = Arc::new(Mutex::new(child));
    let stopping = Arc::new(AtomicBool::new(false));
    let died = Arc::new(AtomicBool::new(false));
    {
        let (child, stopping, died) = (child.clone(), stopping.clone(), died.clone());
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            if stopping.load(Ordering::Acquire) {
                return;
            }
            if child.lock().unwrap().try_wait().ok().flatten().is_some() {
                if !stopping.load(Ordering::Acquire) {
                    died.store(true, Ordering::Release);
                    let _ = ended.send(());
                }
                return;
            }
        });
    }
    Ok(Recorder { program: program.into(), child, stdin, how, path, log, stopping, died })
}

/// An error that says what to install when the program isn't there.
pub fn missing_tool(error: String, tool: &str, package: &str) -> String {
    if error.contains("No such file or directory") {
        crate::i18n::tf("error.record_tool_missing", &[("tool", tool), ("package", package)])
    } else {
        error
    }
}

fn tail(log: &Mutex<String>) -> String {
    let log = log.lock().unwrap();
    let text = log.trim();
    let from = text.len().saturating_sub(600);
    let mut from = from;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    text[from..].to_string()
}

impl Session for Recorder {
    fn stop(mut self: Box<Self>) -> Result<PathBuf, String> {
        self.stopping.store(true, Ordering::Release);
        let id = self.child.lock().unwrap().id();
        let already_gone = self.child.lock().unwrap().try_wait().map_err(|e| e.to_string())?.is_some();
        if !already_gone {
            match self.how {
                Stop::Quit => {
                    if let Some(mut stdin) = self.stdin.take() {
                        let _ = stdin.write_all(b"q\n");
                    }
                }
                Stop::Interrupt => {
                    if let Some(pid) = rustix::process::Pid::from_raw(id as i32) {
                        let _ = rustix::process::kill_process(pid, rustix::process::Signal::INT);
                    }
                }
            }
        }

        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.lock().unwrap().try_wait().map_err(|e| e.to_string())? {
                break Some(status);
            }
            if started.elapsed() > FINISH {
                let _ = self.child.lock().unwrap().kill();
                let _ = self.child.lock().unwrap().wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        // Its last words are in the log once it has closed its end.
        std::thread::sleep(Duration::from_millis(50));

        let written = self.path.metadata().map(|m| m.len()).unwrap_or(0);
        if self.died.load(Ordering::Acquire) {
            eprintln!("queijo: {} stopped by itself ({}): {}", self.program, describe(status), tail(&self.log));
        } else if status.is_none() {
            eprintln!("queijo: {} did not finish in time and was stopped", self.program);
        }
        if written == 0 {
            return Err(format!("{} wrote nothing ({}): {}", self.program, describe(status), tail(&self.log)));
        }
        Ok(self.path.clone())
    }
}

fn describe(status: Option<std::process::ExitStatus>) -> String {
    status.map_or_else(|| "killed".into(), |s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn sh(script: &str, path: &std::path::Path) -> Command {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script).arg("sh").arg(path);
        cmd
    }

    #[test]
    fn stops_a_program_by_typing_q() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("out");
        let (tx, rx) = mpsc::channel();
        // Writes its file when it reads a line, as ffmpeg does when it reads `q`.
        let recorder =
            spawn("fake", sh(r#"read line; echo "$line" > "$1""#, &file), Stop::Quit, file.clone(), tx).unwrap();
        let path = Box::new(recorder).stop().unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap().trim(), "q");
        assert!(rx.try_recv().is_err(), "stopping it is not it ending by itself");
    }

    #[test]
    fn stops_a_program_with_sigint() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("out");
        let (tx, _rx) = mpsc::channel();
        let script = r#"trap 'echo finished > "$1"; exit 0' INT; while true; do sleep 0.05; done"#;
        let recorder = spawn("fake", sh(script, &file), Stop::Interrupt, file.clone(), tx).unwrap();
        let path = Box::new(recorder).stop().unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap().trim(), "finished");
    }

    #[test]
    fn a_program_that_fails_at_once_is_an_error_with_its_message() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, _rx) = mpsc::channel();
        let err = spawn(
            "fake",
            sh(r#"echo "bad geometry" >&2; exit 3"#, &dir.path().join("out")),
            Stop::Quit,
            dir.path().join("out"),
            tx,
        )
        .err()
        .unwrap();
        assert!(err.contains("stopped at once") && err.contains("bad geometry"), "{err}");
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let (tx, _rx) = mpsc::channel();
        let err =
            spawn("queijo-no-such", Command::new("queijo-no-such"), Stop::Quit, "/tmp/x".into(), tx).err().unwrap();
        assert!(err.contains("queijo-no-such"), "{err}");
    }

    #[test]
    fn tells_when_the_recorder_ends_by_itself() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("out");
        let (tx, rx) = mpsc::channel();
        // Stays up past the start-up check, writes something, and goes away on its own.
        let recorder =
            spawn("fake", sh(r#"sleep 1; echo partial > "$1""#, &file), Stop::Quit, file.clone(), tx).unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("it said it had ended");
        // Stopping afterwards still hands back what it wrote.
        assert_eq!(Box::new(recorder).stop().unwrap(), file);
    }

    #[test]
    fn an_empty_result_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("never");
        let (tx, _rx) = mpsc::channel();
        let recorder = spawn("fake", sh("read line", &file), Stop::Quit, file, tx).unwrap();
        let err = Box::new(recorder).stop().unwrap_err();
        assert!(err.contains("wrote nothing"), "{err}");
    }
}
