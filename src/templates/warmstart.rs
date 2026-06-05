use crate::serve::events::BuildEvent;
use crate::serve::process::ServeHandle;
use crate::state::session::{SessionState, WarmState};
use crate::templates::{copy_dir_all, projects_dir, slugify, warm_dir, Template, TemplateKind};
use dioxus::prelude::*;
use std::path::PathBuf;

/// Begin warming a blank template in the background.
///
/// 1. Scaffold a blank project into the warm directory.
/// 2. Spawn `dx serve` on it.
/// 3. Wait for the first `BuildSuccess`.
/// 4. Park the handle in `SessionState.warm_state` so the UI can grab it.
/// 5. Silently drain the event channel so the stdout reader doesn't stall.
pub async fn start_warm_build(mut session: Signal<SessionState>) {
    session.write().warm_state = WarmState::Warming;

    let dir = warm_dir();
    tracing::info!("Warming project at {}", dir.display());

    // Clean previous warm dir and scaffold fresh
    let _ = tokio::fs::remove_dir_all(&dir).await;
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::error!("Failed to create warm dir: {}", e);
        session.write().warm_state = WarmState::Idle;
        return;
    }

    let template = Template::blank();
    if let Err(e) = template.scaffold(&dir).await {
        tracing::error!("Failed to scaffold warm project: {}", e);
        session.write().warm_state = WarmState::Idle;
        return;
    }

    let dioxus_toml = r#"[application]
name = "warm"
default_platform = "desktop"
"#;
    if let Err(e) = tokio::fs::write(dir.join("Dioxus.toml"), dioxus_toml).await {
        tracing::error!("Failed to write Dioxus.toml: {}", e);
        session.write().warm_state = WarmState::Idle;
        return;
    }

    match ServeHandle::spawn(dir.clone()).await {
        Ok((handle, mut event_rx)) => {
            tracing::info!("Warm dx serve spawned, waiting for BuildSuccess...");

            let mut saw_success = false;
            while let Some(event) = event_rx.recv().await {
                if matches!(event, BuildEvent::BuildSuccess) {
                    saw_success = true;
                    break;
                }
            }

            if !saw_success {
                tracing::warn!("Warm build never succeeded — giving up");
                let _ = handle.kill();
                session.write().warm_state = WarmState::Idle;
                return;
            }

            tracing::info!("Warm build is green — ready for take-off");

            // Spawn a silent drain so the stdout reader doesn't backpressure
            tokio::spawn(async move {
                while event_rx.recv().await.is_some() {}
            });

            session.write().warm_state = WarmState::Ready {
                project_path: dir,
                serve_handle: handle,
            };
        }
        Err(e) => {
            tracing::error!("Failed to spawn warm dx serve: {}", e);
            session.write().warm_state = WarmState::Idle;
        }
    }
}

/// Consume the warm project, copy it to a user project directory, inject the
/// chosen template files, and start a fresh `dx serve` there.
///
/// Returns the path of the newly-created project.
pub async fn take_warm_project(
    mut session: Signal<SessionState>,
    kind: TemplateKind,
    description: String,
) -> Option<PathBuf> {
    let warm = {
        let mut s = session.write();
        match std::mem::replace(&mut s.warm_state, WarmState::Taken) {
            WarmState::Ready {
                project_path,
                serve_handle,
            } => {
                // Kill the warm dx serve before we touch its files
                serve_handle.kill();
                project_path
            }
            other => {
                s.warm_state = other;
                return None;
            }
        }
    };

    // Immediately start the next warm project in the background
    let session_clone = session;
    dioxus::prelude::spawn(async move {
        // Brief pause so the OS releases file handles from the killed process
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        start_warm_build(session_clone).await;
    });

    // Create user project directory
    let slug = slugify(&description);
    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("{}", secs)
    };
    let project_name = if slug.is_empty() {
        format!("{}", now)
    } else {
        format!("{}-{}", now, slug)
    };
    let dest = projects_dir().join(&project_name);

    tracing::info!("Copying warm project to {}", dest.display());

    if let Err(e) = copy_dir_all(&warm, &dest).await {
        tracing::error!("Failed to copy warm project: {}", e);
        return None;
    }

    // Inject the selected template files (overwrites blank scaffold)
    let template = match kind {
        TemplateKind::Blank => Template::blank(),
        TemplateKind::Store => Template::store(),
        // Fall back to blank for unimplemented templates
        _ => Template::blank(),
    };

    if let Err(e) = template.scaffold(&dest).await {
        tracing::error!("Failed to inject template files: {}", e);
        return None;
    }

    tracing::info!("Project ready at {}", dest.display());
    Some(dest)
}
