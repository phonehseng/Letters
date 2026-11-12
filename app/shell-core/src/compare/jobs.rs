//! every comparison running or shown, by window.
//!
//! each job gets its own folder under the app's cache, for the copy of an
//! open document taken when the comparison starts (so later edits can't
//! change what is being compared half way through). the folders are
//! deleted when a job closes; any left by a crash are deleted at the next
//! start, since only one copy of Letters runs at a time.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::job::CompareJob;

pub struct CompareJobs {
    root: PathBuf,
    next_id: AtomicU64,
    /// job id → (the window it belongs to, the job)
    jobs: Mutex<HashMap<u64, (String, Arc<CompareJob>)>>,
}

impl CompareJobs {
    /// also deletes folders a crashed run left behind.
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&root);
        Self {
            root,
            next_id: AtomicU64::new(1),
            jobs: Mutex::new(HashMap::new()),
        }
    }

    /// a new job id with an empty folder of its own.
    pub fn prepare(&self) -> Result<(u64, PathBuf), String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let folder = self.root.join(format!("comparison-{id}"));
        fs::create_dir_all(&folder).map_err(|error| {
            format!("Letters can’t make room for the comparison. Check the free space on this computer, then try again. ({error})")
        })?;
        Ok((id, folder))
    }

    pub fn insert(&self, window: &str, job: CompareJob) -> Arc<CompareJob> {
        let job = Arc::new(job);
        self.lock()
            .insert(job.id, (window.to_owned(), Arc::clone(&job)));
        job
    }

    pub fn get(&self, id: u64) -> Result<Arc<CompareJob>, String> {
        self.lock()
            .get(&id)
            .map(|(_, job)| Arc::clone(job))
            .ok_or_else(|| {
                "This comparison is no longer open. Compare the documents again.".to_owned()
            })
    }

    /// closes a job: its documents leave the worker and its folder is deleted.
    pub fn close(&self, id: u64) {
        let removed = self.lock().remove(&id);
        if let Some((_, job)) = removed {
            job.close();
        }
    }

    /// closes every job of a window that is going away.
    pub fn close_window(&self, window: &str) {
        let ids: Vec<u64> = self
            .lock()
            .iter()
            .filter(|(_, (owner, _))| owner == window)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.close(id);
        }
    }

    pub fn open_count(&self) -> usize {
        self.lock().len()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, (String, Arc<CompareJob>)>> {
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
