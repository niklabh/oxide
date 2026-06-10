//! Per-origin permission model for sensitive host APIs.
//!
//! Sensitive capabilities (camera, microphone, geolocation, screen capture) are not granted
//! automatically. The first time an app requests one, the host queues a [`PendingPermission`]
//! and the UI shell renders a Chrome-style prompt (top-left, under the toolbar). Until the
//! user decides, the gated API returns a "pending" code ([`PERMISSION_PENDING`] for `i32`
//! APIs) and the guest is expected to retry on a later frame.
//!
//! Decisions are remembered per `(origin, kind)` pair for the lifetime of the tab, where
//! *origin* is derived via [`crate::url::app_origin_of`] (scheme + host + port for network
//! URLs, the containing directory for `file://` URLs).
//!
//! This flow is deliberately non-blocking: guest frame callbacks run on the UI thread, so a
//! modal prompt inside a host function would deadlock the renderer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Return code for permission-gated `i32` host APIs while the prompt is awaiting a decision.
///
/// Guests should treat this as "try again next frame", not as a hard failure. Mirrored as
/// `oxide_sdk::PERMISSION_PENDING`.
pub const PERMISSION_PENDING: i32 = -5;

/// A sensitive capability that requires an explicit user grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PermissionKind {
    Camera,
    Microphone,
    Geolocation,
    ScreenCapture,
}

impl PermissionKind {
    /// Human-readable request line shown in the permission prompt.
    pub fn description(&self) -> &'static str {
        match self {
            PermissionKind::Camera => "Use your camera",
            PermissionKind::Microphone => "Use your microphone",
            PermissionKind::Geolocation => "Know your location",
            PermissionKind::ScreenCapture => "See your screen",
        }
    }

    /// Stable identifier used in manifests and logs.
    pub fn name(&self) -> &'static str {
        match self {
            PermissionKind::Camera => "camera",
            PermissionKind::Microphone => "microphone",
            PermissionKind::Geolocation => "geolocation",
            PermissionKind::ScreenCapture => "screen-capture",
        }
    }
}

/// Outcome of a permission check from a host function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionStatus {
    /// The user allowed this `(origin, kind)`; proceed.
    Granted,
    /// The user blocked this `(origin, kind)`; fail the call.
    Denied,
    /// A prompt is showing (or queued); the guest should retry on a later frame.
    Pending,
}

/// A permission request awaiting a user decision, rendered by the UI shell.
#[derive(Clone, Debug)]
pub struct PendingPermission {
    /// App origin making the request (see [`crate::url::app_origin_of`]).
    pub origin: String,
    /// Capability being requested.
    pub kind: PermissionKind,
}

/// Per-tab permission decisions plus the prompt currently awaiting the user.
#[derive(Default)]
pub struct PermissionsState {
    /// `(origin, kind)` → allowed. Absent means "not yet asked".
    decisions: HashMap<(String, PermissionKind), bool>,
    /// At most one prompt is shown at a time; further requests stay [`PermissionStatus::Pending`]
    /// and re-queue themselves on retry once this one resolves.
    pub pending: Option<PendingPermission>,
}

/// Shared handle stored in `HostState` and read by the UI shell each frame.
pub type SharedPermissions = Arc<Mutex<PermissionsState>>;

/// Looks up the decision for `(origin, kind)`, queueing a prompt when undecided.
///
/// Returns [`PermissionStatus::Pending`] both when this request becomes the active prompt and
/// when another prompt is already showing (the retry will queue it once the slot frees up).
pub fn check_or_request(
    perms: &SharedPermissions,
    origin: &str,
    kind: PermissionKind,
) -> PermissionStatus {
    let mut state = perms.lock().unwrap();
    if let Some(&allowed) = state.decisions.get(&(origin.to_string(), kind)) {
        return if allowed {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        };
    }
    if state.pending.is_none() {
        state.pending = Some(PendingPermission {
            origin: origin.to_string(),
            kind,
        });
    }
    PermissionStatus::Pending
}

/// Records the user's decision for the active prompt and dismisses it.
///
/// Called by the UI shell when Allow/Block is clicked. No-op when nothing is pending.
pub fn resolve_pending(perms: &SharedPermissions, allow: bool) {
    let mut state = perms.lock().unwrap();
    if let Some(req) = state.pending.take() {
        state.decisions.insert((req.origin, req.kind), allow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared() -> SharedPermissions {
        Arc::new(Mutex::new(PermissionsState::default()))
    }

    #[test]
    fn first_request_is_pending_and_queues_prompt() {
        let perms = shared();
        let status = check_or_request(&perms, "https://a.com", PermissionKind::Camera);
        assert_eq!(status, PermissionStatus::Pending);
        let state = perms.lock().unwrap();
        let pending = state.pending.as_ref().expect("prompt queued");
        assert_eq!(pending.origin, "https://a.com");
        assert_eq!(pending.kind, PermissionKind::Camera);
    }

    #[test]
    fn allow_then_granted() {
        let perms = shared();
        check_or_request(&perms, "https://a.com", PermissionKind::Camera);
        resolve_pending(&perms, true);
        assert_eq!(
            check_or_request(&perms, "https://a.com", PermissionKind::Camera),
            PermissionStatus::Granted
        );
    }

    #[test]
    fn block_then_denied() {
        let perms = shared();
        check_or_request(&perms, "https://a.com", PermissionKind::Microphone);
        resolve_pending(&perms, false);
        assert_eq!(
            check_or_request(&perms, "https://a.com", PermissionKind::Microphone),
            PermissionStatus::Denied
        );
    }

    #[test]
    fn decisions_are_scoped_per_origin_and_kind() {
        let perms = shared();
        check_or_request(&perms, "https://a.com", PermissionKind::Camera);
        resolve_pending(&perms, true);
        // Same origin, different kind: must prompt again.
        assert_eq!(
            check_or_request(&perms, "https://a.com", PermissionKind::Microphone),
            PermissionStatus::Pending
        );
        resolve_pending(&perms, false);
        // Different origin, same kind: must prompt again.
        assert_eq!(
            check_or_request(&perms, "https://b.com", PermissionKind::Camera),
            PermissionStatus::Pending
        );
    }

    #[test]
    fn second_request_waits_for_active_prompt() {
        let perms = shared();
        check_or_request(&perms, "https://a.com", PermissionKind::Camera);
        // Another kind requested while the camera prompt is up: stays pending, not queued.
        assert_eq!(
            check_or_request(&perms, "https://a.com", PermissionKind::Geolocation),
            PermissionStatus::Pending
        );
        assert_eq!(
            perms.lock().unwrap().pending.as_ref().unwrap().kind,
            PermissionKind::Camera
        );
        resolve_pending(&perms, true);
        // Retry now queues the geolocation prompt.
        assert_eq!(
            check_or_request(&perms, "https://a.com", PermissionKind::Geolocation),
            PermissionStatus::Pending
        );
        assert_eq!(
            perms.lock().unwrap().pending.as_ref().unwrap().kind,
            PermissionKind::Geolocation
        );
    }

    #[test]
    fn resolve_without_pending_is_noop() {
        let perms = shared();
        resolve_pending(&perms, true);
        assert!(perms.lock().unwrap().pending.is_none());
    }
}
