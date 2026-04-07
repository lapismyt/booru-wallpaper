use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::types::get_default_cache_dir_path;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

pub struct CachedTempDir {
    path: PathBuf,
}

impl CachedTempDir {
    pub fn new(prefix: &str) -> anyhow::Result<Self> {
        let base_dir = get_default_cache_dir_path();
        for _ in 0..32 {
            let path = base_dir.join(unique_name(prefix));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(anyhow::anyhow!(
                        "Failed to create cache temp dir {}: {}",
                        path.display(),
                        error
                    ));
                }
            }
        }

        Err(anyhow::anyhow!(
            "Failed to allocate a unique cache temp dir"
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for CachedTempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{}-{}-{}-{}", prefix, pid, nanos, id)
}
