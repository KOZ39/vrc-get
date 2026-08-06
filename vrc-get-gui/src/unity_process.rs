use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::path::{Component, Prefix};

use sysinfo::{Process, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Clone, Debug)]
pub(crate) struct UnityProcess {
    pub(crate) project_path: PathBuf,
    pub(crate) process_id: u32,
}

#[cfg(target_os = "macos")]
pub(crate) fn find_unity_process_ids_for_project(project_path: &Path) -> Vec<u32> {
    let mut system = System::new();
    refresh_unity_processes(&mut system)
        .into_iter()
        .filter_map(|process| {
            paths_match(&process.project_path, project_path).then_some(process.process_id)
        })
        .collect()
}

pub(crate) fn refresh_unity_processes(system: &mut System) -> Vec<UnityProcess> {
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .without_tasks(),
    );

    system
        .processes()
        .values()
        .filter_map(|process| {
            if !is_unity_process(process) {
                return None;
            }

            let process_project_path = process.cwd()?;
            Some(UnityProcess {
                project_path: process_project_path.to_owned(),
                process_id: process.pid().as_u32(),
            })
        })
        .collect()
}

fn is_unity_process(process: &Process) -> bool {
    process
        .exe()
        .and_then(Path::file_stem)
        .is_some_and(is_unity_name)
}

fn is_unity_name(name: &OsStr) -> bool {
    name.eq_ignore_ascii_case(OsStr::new("Unity"))
}

pub(crate) fn paths_match(left: &Path, right: &Path) -> bool {
    let left = comparable_path(left);
    let right = comparable_path(right);

    #[cfg(windows)]
    {
        left.as_os_str().eq_ignore_ascii_case(right.as_os_str())
    }

    #[cfg(not(windows))]
    {
        left == right
    }
}

fn comparable_path(path: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));

    #[cfg(windows)]
    {
        normalize_windows_path(&canonical)
    }

    #[cfg(not(windows))]
    {
        canonical
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: &Path) -> PathBuf {
    let mut components = path.components();
    let Some(first) = components.next() else {
        return PathBuf::from(path);
    };

    std::iter::once(normalize_first_component(first))
        .chain(components.map(|component| PathBuf::from(component.as_os_str())))
        .collect()
}

#[cfg(windows)]
fn normalize_first_component(component: Component<'_>) -> PathBuf {
    let Component::Prefix(prefix) = component else {
        return PathBuf::from(component.as_os_str());
    };

    match prefix.kind() {
        Prefix::VerbatimDisk(disk) | Prefix::Disk(disk) => {
            PathBuf::from(format!("{}:", char::from(disk)))
        }
        Prefix::VerbatimUNC(server, share) | Prefix::UNC(server, share) => {
            let mut path = PathBuf::from(r"\\");
            path.push(server);
            path.push(share);
            path
        }
        _ => PathBuf::from(prefix.as_os_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_unity_executable_name_without_lossy_conversion() {
        assert!(is_unity_name(OsStr::new("Unity")));
        assert!(is_unity_name(OsStr::new("UNITY")));
        assert!(!is_unity_name(OsStr::new("UnityShaderCompiler")));
    }

    #[test]
    #[cfg(windows)]
    fn matches_windows_paths_case_insensitively_and_ignores_separator_differences() {
        assert!(paths_match(
            Path::new(r"C:\Projects\Avatar"),
            Path::new(r"c:/projects/avatar/")
        ));
    }

    #[test]
    #[cfg(windows)]
    fn matches_verbatim_disk_paths() {
        assert!(paths_match(
            Path::new(r"\\?\C:\Projects\Avatar"),
            Path::new(r"C:\Projects\Avatar")
        ));
    }

    #[test]
    #[cfg(windows)]
    fn matches_verbatim_unc_paths() {
        assert!(paths_match(
            Path::new(r"\\?\UNC\server\share\Avatar"),
            Path::new(r"\\server\share\Avatar")
        ));
    }
}
