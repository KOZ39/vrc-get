use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(windows)]
use sysinfo::System;

const UNITY_OPENING_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(windows)]
const UNITY_RUNTIME_CACHE_TTL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct UnityProjectState {
    opening: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    #[cfg(windows)]
    runtime_cache: Arc<Mutex<UnityRuntimeCache>>,
}

#[cfg(windows)]
struct UnityRuntimeCache {
    system: System,
    refreshed_at: Option<Instant>,
    editor_ready_projects: Vec<PathBuf>,
}

impl UnityProjectState {
    pub fn new() -> Self {
        Self {
            opening: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(windows)]
            runtime_cache: Arc::new(Mutex::new(UnityRuntimeCache::new())),
        }
    }

    pub fn try_mark_opening(&self, project_path: impl Into<PathBuf>) -> bool {
        self.try_mark_opening_at(project_path.into(), Instant::now())
    }

    pub fn is_opening(&self, project_path: &Path) -> bool {
        self.is_opening_at(project_path, Instant::now())
    }

    pub fn clear_opening(&self, project_path: &Path) {
        self.opening.lock().unwrap().remove(project_path);
    }

    pub fn is_editor_ready(&self, project_path: &Path) -> bool {
        #[cfg(windows)]
        {
            return self
                .runtime_cache
                .lock()
                .unwrap()
                .is_editor_ready(project_path, Instant::now());
        }

        #[cfg(not(windows))]
        {
            let _ = project_path;
            false
        }
    }

    fn try_mark_opening_at(&self, project_path: PathBuf, now: Instant) -> bool {
        let mut opening = self.opening.lock().unwrap();
        remove_expired(&mut opening, now);

        if opening.contains_key(&project_path) {
            return false;
        }

        opening.insert(project_path, now);
        drop(opening);
        self.invalidate_runtime_cache();
        true
    }

    fn is_opening_at(&self, project_path: &Path, now: Instant) -> bool {
        let mut opening = self.opening.lock().unwrap();
        remove_expired(&mut opening, now);
        opening.contains_key(project_path)
    }

    fn invalidate_runtime_cache(&self) {
        #[cfg(windows)]
        self.runtime_cache.lock().unwrap().invalidate();
    }
}

#[cfg(windows)]
impl UnityRuntimeCache {
    fn new() -> Self {
        Self {
            system: System::new(),
            refreshed_at: None,
            editor_ready_projects: Vec::new(),
        }
    }

    fn is_editor_ready(&mut self, project_path: &Path, now: Instant) -> bool {
        if self.should_refresh(now) {
            self.refresh(now);
        }

        self.editor_ready_projects
            .iter()
            .any(|ready_project| crate::unity_process::paths_match(ready_project, project_path))
    }

    fn should_refresh(&self, now: Instant) -> bool {
        self.refreshed_at.is_none_or(|refreshed_at| {
            now.saturating_duration_since(refreshed_at) >= UNITY_RUNTIME_CACHE_TTL
        })
    }

    fn refresh(&mut self, now: Instant) {
        let processes = crate::unity_process::refresh_unity_processes(&mut self.system);
        self.editor_ready_projects = match crate::os::find_unity_editor_ready_projects(processes) {
            Ok(projects) => projects,
            Err(error) => {
                log::debug!("Checking which Unity editor windows are ready: {error}");
                Vec::new()
            }
        };
        self.refreshed_at = Some(now);
    }

    fn invalidate(&mut self) {
        self.refreshed_at = None;
        self.editor_ready_projects.clear();
    }
}

fn remove_expired(opening: &mut HashMap<PathBuf, Instant>, now: Instant) {
    opening.retain(|_, started_at| now.duration_since(*started_at) < UNITY_OPENING_TIMEOUT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_marks_a_project_opening_once() {
        let state = UnityProjectState::new();
        let project = PathBuf::from("project");

        assert!(state.try_mark_opening(project.clone()));
        assert!(!state.try_mark_opening(project));
    }

    #[test]
    fn clearing_allows_a_project_to_be_marked_again() {
        let state = UnityProjectState::new();
        let project = PathBuf::from("project");

        assert!(state.try_mark_opening(project.clone()));
        state.clear_opening(&project);

        assert!(state.try_mark_opening(project));
    }

    #[test]
    fn opening_state_expires() {
        let state = UnityProjectState::new();
        let project = PathBuf::from("project");
        let started_at = Instant::now();

        assert!(state.try_mark_opening_at(project.clone(), started_at));
        assert!(state.is_opening_at(
            &project,
            started_at + UNITY_OPENING_TIMEOUT - Duration::from_millis(1)
        ));
        assert!(!state.is_opening_at(&project, started_at + UNITY_OPENING_TIMEOUT));
    }

    #[test]
    #[cfg(windows)]
    fn runtime_cache_expires_after_its_ttl() {
        let mut cache = UnityRuntimeCache::new();
        let refreshed_at = Instant::now();
        cache.refreshed_at = Some(refreshed_at);

        assert!(
            !cache
                .should_refresh(refreshed_at + UNITY_RUNTIME_CACHE_TTL - Duration::from_millis(1))
        );
        assert!(cache.should_refresh(refreshed_at + UNITY_RUNTIME_CACHE_TTL));
    }

    #[test]
    #[cfg(windows)]
    fn starting_a_new_launch_invalidates_runtime_cache() {
        let state = UnityProjectState::new();
        let project = PathBuf::from("project");

        {
            let mut cache = state.runtime_cache.lock().unwrap();
            cache.refreshed_at = Some(Instant::now());
            cache.editor_ready_projects.push(project.clone());
        }

        assert!(state.try_mark_opening(project.clone()));

        let cache = state.runtime_cache.lock().unwrap();
        assert!(cache.refreshed_at.is_none());
        assert!(cache.editor_ready_projects.is_empty());
    }
}
