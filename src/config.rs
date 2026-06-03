use std::path::PathBuf;

fn last_project_file() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("rocky").join("last_project.txt")
}

/// Walk up from `path` until we find a directory containing `Cargo.toml`.
/// Returns that directory, or the original path if none found.
pub fn find_project_root(path: &PathBuf) -> PathBuf {
    let start = if path.is_file() {
        path.parent().map(PathBuf::from).unwrap_or(path.clone())
    } else {
        path.clone()
    };

    let mut current = start.clone();
    loop {
        if current.join("Cargo.toml").exists() {
            return current;
        }
        match current.parent() {
            Some(parent) => current = parent.to_path_buf(),
            None => return start, // reached filesystem root, give up
        }
    }
}

pub fn save_last_project(path: &PathBuf) {
    let root = find_project_root(path);
    let file = last_project_file();
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&file, root.to_string_lossy().as_bytes());
}

pub fn load_last_project() -> Option<PathBuf> {
    let content = std::fs::read_to_string(last_project_file()).ok()?;
    let path = find_project_root(&PathBuf::from(content.trim()));
    if path.exists() { Some(path) } else { None }
}
