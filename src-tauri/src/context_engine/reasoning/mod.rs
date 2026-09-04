pub mod models;
pub mod traits;
pub mod mock;
pub mod builder;
pub mod validator;
pub mod fixtures;
pub mod provider;
pub mod remote;
pub mod privacy;

#[cfg(test)]
pub mod tests;

pub use models::*;
pub use traits::AIReasoner;
pub use mock::MockAIReasoner;
pub use builder::{build_evidence_package, analyze_item_context_ai, EvidencePackageBuilder};
pub use privacy::{AIPrivacyBoundary, AIPrivacyPolicy, AIPrivacyViolation, scrub_private_data};
pub use validator::{
    detect_executable_instructions, detect_exposed_internal_metrics, parse_and_validate_response,
    validate_explanation_grounding, validate_reasoning_result, ReasoningValidationError,
};
pub use fixtures::*;
pub use provider::{
    create_ai_reasoner, create_default_ai_reasoner, get_reasoner_status, AIProviderConfig,
    AIProviderStatus, AIProviderType,
};
pub use remote::RemoteLLMReasoner;

