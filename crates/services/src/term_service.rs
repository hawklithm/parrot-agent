//! TermService — manages feedback data sharing terms version.
//!
//! Mirrors Paperclip's `DEFAULT_FEEDBACK_DATA_SHARING_TERMS_VERSION` constant
//! and provides the default terms version string used when a Board user enables
//! feedback data sharing for a company.

/// Default feedback data sharing terms version (mirrors Paperclip).
pub const DEFAULT_FEEDBACK_DATA_SHARING_TERMS_VERSION: &str = "feedback-data-sharing-v1";

/// Service for managing feedback data sharing terms.
///
/// In Paperclip, there is no standalone `/terms` API — the terms version is a
/// field on the Company model.  This service provides the canonical default
/// version and is consumed by the company update handler when
/// `feedbackDataSharingEnabled` transitions to `true`.
#[async_trait::async_trait]
pub trait TermService: Send + Sync {
    /// Returns the current default terms version string.
    fn default_terms_version(&self) -> &'static str;
}

/// Default implementation of [`TermService`].
#[derive(Debug, Clone, Default)]
pub struct DefaultTermService;

impl DefaultTermService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl TermService for DefaultTermService {
    fn default_terms_version(&self) -> &'static str {
        DEFAULT_FEEDBACK_DATA_SHARING_TERMS_VERSION
    }
}
