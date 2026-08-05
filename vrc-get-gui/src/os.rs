//! OS-specific functionality.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum BringUnityToFrontResult {
    BroughtToFront,
    AttentionRequested,
    WindowNotFound,
    Unsupported,
}

pub(crate) const CAN_BRING_UNITY_TO_FRONT: bool = cfg!(any(windows, target_os = "macos"));
pub(crate) const CAN_DETECT_UNITY_EDITOR_READY: bool = cfg!(windows);

#[cfg(windows)]
#[path = "os_windows.rs"]
mod platform;

#[cfg(not(windows))]
#[path = "os_posix.rs"]
mod platform;

pub(crate) use platform::*;
