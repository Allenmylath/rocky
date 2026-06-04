use crate::serve::events::BuildEvent;
use crate::serve::parser::StderrParser;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::mpsc;

/// Handle to a running `dx serve` child process.
///
/// Dropping this handle **does not** kill the process — call [`ServeHandle::kill`]
/// explicitly when you want to shut it down.
pub struct ServeHandle {
    kill_tx: tokio::sync::oneshot::Sender<()>,
    stdin: Option<ChildStdin>,
}

impl ServeHandle {
    /// Spawn `dx serve` in the given project directory.
    /// Returns a handle and a receiver for build events.
    pub async fn spawn(
        project_path: PathBuf,
    ) -> anyhow::Result<(Self, mpsc::Receiver<BuildEvent>)> {
        // Bug 3: increased from 128 to 1024 so bursts of diagnostics / raw log lines
        // don't apply backpressure and stall the OS pipe buffers.
        let (event_tx, event_rx) = mpsc::channel::<BuildEvent>(1024);
        let (kill_tx, mut kill_rx) = tokio::sync::oneshot::channel::<()>();

        // Spawn dx serve as a child process
        let mut child = Command::new("dx")
            .arg("serve")
            .current_dir(&project_path)
            // Bug 4: pipe stdin so we can send "r\n" to trigger rebuilds
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn `dx serve`: {}. Is dx installed?", e))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let tx_clone = event_tx.clone();
        tokio::spawn(async move {
            // Merge stdout and stderr lines into a single channel so EOF on one
            // doesn't starve the other.
            let (line_tx, mut line_rx) = mpsc::channel::<String>(256);

            // stdout reader (Bug 5: keep the JoinHandle so we can abort it)
            let lt = line_tx.clone();
            let h1 = tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if lt.send(l).await.is_err() {
                        break;
                    }
                }
            });

            // stderr reader (Bug 5: keep the JoinHandle so we can abort it)
            let h2 = tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    if line_tx.send(l).await.is_err() {
                        break;
                    }
                }
            });

            let mut parser = StderrParser::new();
            let mut killed = false;

            tokio::select! {
                // Read merged lines until both stdout and stderr hit EOF.
                _ = async {
                    while let Some(line) = line_rx.recv().await {
                        for event in parser.feed_line(&line) {
                            if tx_clone.send(event).await.is_err() {
                                return;
                            }
                        }
                    }
                } => {
                    // Natural EOF — child has exited on its own.
                }
                // Kill signal from ServeHandle::kill
                _ = &mut kill_rx => {
                    let _ = child.kill().await;
                    killed = true;
                }
            }

            // Bug 5: abort the reader tasks so they don't leak if we exited via
            // the kill branch before EOF, or if the event channel was closed.
            h1.abort();
            h2.abort();

            // Bug 1: if we exited because the event channel was closed (consumer
            // dropped), the child may still be alive. Make sure it's dead before
            // we call wait().
            if !killed {
                let _ = child.kill().await;
            }

            // Bug 2: flush any in-progress diagnostic regardless of which branch
            // we took.  This ensures partially-accumulated rustc errors are not
            // lost when the user hits Kill.
            for event in parser.flush() {
                let _ = tx_clone.send(event).await;
            }

            // Wait for the child to fully exit and emit the final event.
            let code = match child.wait().await {
                Ok(status) => status.code(),
                Err(_) => None,
            };
            let _ = tx_clone.send(BuildEvent::ProcessExited { code }).await;
        });

        Ok((Self { kill_tx, stdin }, event_rx))
    }

    /// Send `r\n` to dx serve's stdin to trigger a hot reload without
    /// killing and respawning the process.
    pub async fn restart(&mut self) -> anyhow::Result<()> {
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(b"r\n").await?;
            stdin.flush().await?;
            Ok(())
        } else {
            anyhow::bail!("stdin not available — was the process spawned correctly?")
        }
    }

    /// Kill the `dx serve` process gracefully.
    pub fn kill(self) {
        let _ = self.kill_tx.send(());
    }
}

