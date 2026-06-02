use crate::serve::events::BuildEvent;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// Handle to a running `dx serve` child process
pub struct ServeHandle {
    /// Kill signal sender — drop or send to stop the process
    _kill_tx: tokio::sync::oneshot::Sender<()>,
}

impl ServeHandle {
    /// Spawn `dx serve` in the given project directory.
    /// Returns a handle and a receiver for build events.
    pub async fn spawn(
        project_path: PathBuf,
    ) -> anyhow::Result<(Self, mpsc::Receiver<BuildEvent>)> {
        // TODO: implement in serve/process.rs task
        let (_kill_tx, _kill_rx) = tokio::sync::oneshot::channel();
        let (_tx, rx) = mpsc::channel(64);
        Ok((Self { _kill_tx }, rx))
    }
}