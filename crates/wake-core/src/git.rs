use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub branch: Option<String>,
    pub root: Option<PathBuf>,
}

pub struct GitCache {
    cache: HashMap<PathBuf, GitInfo>,
}

impl GitCache {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

    pub fn get(&mut self, cwd: &Path) -> GitInfo {
        if let Some(info) = self.cache.get(cwd) {
            return info.clone();
        }

        let info = GitInfo { branch: Self::detect_branch(cwd), root: Self::detect_root(cwd) };
        self.cache.insert(cwd.to_path_buf(), info.clone());
        info
    }

    fn detect_branch(cwd: &Path) -> Option<String> {
        Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(cwd)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }

    fn detect_root(cwd: &Path) -> Option<PathBuf> {
        Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(cwd)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
    }
}

impl Default for GitCache {
    fn default() -> Self {
        Self::new()
    }
}
