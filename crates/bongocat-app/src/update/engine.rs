//! What the worker is allowed to ask of the update subsystem.
//!
//! Signed installation and read-only Together notifications share the state
//! machine. Each engine explicitly declares whether installation is supported.

use super::*;

/// The update pipeline the worker drives.
///
/// The worker's own job is the state machine around the pipeline: which phase is
/// published when, which failures are attributed to which stage, and what happens to
/// the phase the window is rendering. Depending on this trait rather than on
/// [`UpdateRuntime`] directly is what lets that state machine be exercised without a
/// network, a published release or a real install.
pub(crate) trait UpdateEngine: Send + 'static {
    fn supports_install(&self) -> bool;
    fn unavailability(&self) -> Option<UpdateUnavailability>;
    fn release_page_url(&self, version: &str) -> Option<String>;
    fn check(&self) -> Result<UpdateOutcome, UpdateError>;
    fn install(&self, observe: &dyn Fn(UpdateEvent)) -> Result<UpdateOutcome, UpdateError>;
}

impl UpdateEngine for UpdateRuntime {
    fn supports_install(&self) -> bool {
        true
    }

    fn unavailability(&self) -> Option<UpdateUnavailability> {
        UpdateRuntime::unavailability(self)
    }

    fn release_page_url(&self, version: &str) -> Option<String> {
        UpdateRuntime::release_page_url(self, version)
    }

    fn check(&self) -> Result<UpdateOutcome, UpdateError> {
        UpdateRuntime::check(self)
    }

    fn install(&self, observe: &dyn Fn(UpdateEvent)) -> Result<UpdateOutcome, UpdateError> {
        self.install_with_observer(observe)
    }
}

impl UpdateEngine for TogetherReleaseChecker {
    fn supports_install(&self) -> bool {
        false
    }

    fn unavailability(&self) -> Option<UpdateUnavailability> {
        None
    }

    fn release_page_url(&self, version: &str) -> Option<String> {
        TogetherReleaseChecker::release_page_url(version)
    }

    fn check(&self) -> Result<UpdateOutcome, UpdateError> {
        TogetherReleaseChecker::check(self)
    }

    fn install(&self, _: &dyn Fn(UpdateEvent)) -> Result<UpdateOutcome, UpdateError> {
        Err(UpdateError::at(
            bongocat_update::UpdateStage::Install,
            bongocat_update::UpdateErrorCode::SignatureKeyMissing,
        ))
    }
}
