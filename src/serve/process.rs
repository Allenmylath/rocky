use crate::serve::events::BuildEvent;
use crate::serve::parser::StderrParser;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// Handle to a running `dx serve` child process.
/// Dropping this handle kills the process.
pub struct ServeHandle {
    kill_tx: tokio::sync::oneshot::Sender<()>,
}

impl ServeHandle {
    /// Spawn `dx serve` in the given project directory.
    /// Returns a handle and a receiver for build events.
    pub async fn spawn(
        project_path: PathBuf,
    ) -> anyhow::Result<(Self, mpsc::Receiver<BuildEvent>)> {
        let (event_tx, event_rx) = mpsc::channel::<BuildEvent>(128);
        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

        // Spawn dx serve as a child process
        let mut child = Command::new("dx")
            .arg("serve")
            .current_dir(&project_path)
            // Merge stderr into stdout so we read one stream
            // dx serve writes everything to stderr, stdout is mostly empty
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn `dx serve`: {}. Is dx installed?", e))?;

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        // Spawn the reader task
        let tx_clone = event_tx.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = read_output(stdout, stderr, tx_clone.clone()) => {}
                _ = kill_rx => {
                    // Kill signal received — terminate the child
                    let _ = child.kill().await;
                }
            }

            // Child exited — send the exit event
            let code = match child.wait().await {
                Ok(status) => status.code(),
                Err(_) => None,
            };
            let _ = tx_clone.send(BuildEvent::ProcessExited { code }).await;
        });

        Ok((Self { kill_tx }, event_rx))
    }

    /// Kill the `dx serve` process
    pub fn kill(self) {
        let _ = self.kill_tx.send(());
    }
}

/// Read stdout and stderr concurrently, parse each line, send events
async fn read_output(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    tx: mpsc::Sender<BuildEvent>,
) {
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // dx serve writes diagnostics to stderr and status to stdout
    // We read both concurrently and merge into the same parser
    // since dx mixes them based on version/platform
    let mut parser = StderrParser::new();

    loop {
        tokio::select! {
            // stdout line
            line = stdout_reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        let events = parser.feed_line(&l);
                        for event in events {
                            if tx.send(event).await.is_err() {
                                return; // receiver dropped
                            }
                        }
                    }
                    Ok(None) => break, // EOF
                    Err(_) => break,
                }
            }
            // stderr line
            line = stderr_reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        let events = parser.feed_line(&l);
                        for event in events {
                            if tx.send(event).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
    }

    // Flush any in-progress diagnostic at EOF
    for event in parser.flush() {
        let _ = tx.send(event).await;
    }
}