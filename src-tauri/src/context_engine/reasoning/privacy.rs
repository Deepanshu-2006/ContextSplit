use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::models::EvidencePackage;
use crate::db::settings::PrivacySettings;

/// Policy controlling what information may be sent to an AI reasoning provider.
/// Defaults to strict data minimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIPrivacyPolicy {
    /// Master toggle: whether remote AI reasoning is allowed.
    /// When false, remote AI reasoning is completely blocked, and ContextSplit
    /// operates exclusively on deterministic Context Engine logic.
    pub enable_ai_reasoning: bool,

    /// Whether active application metadata may be included.
    pub allow_active_app: bool,

    /// Whether browser domain and URL metadata may be included.
    pub allow_browser_domain: bool,

    /// Whether session IDs and session timeline context may be included.
    pub allow_session_tracking: bool,

    /// Hard upper bound on related items sent in evidence (default: 3).
    /// Strictly prevents full clipboard history dumps.
    pub max_related_items: usize,

    /// Maximum character length for any content snippet (default: 200).
    pub max_snippet_chars: usize,

    /// Minimum relationship strength threshold for related items (default: 0.40).
    /// Unrelated items below this threshold are strictly excluded.
    pub min_relationship_strength: f64,

    /// Whether deep secret, credential, and token scrubbing is active (default: true).
    pub scrub_secrets: bool,
}

impl Default for AIPrivacyPolicy {
    fn default() -> Self {
        Self {
            enable_ai_reasoning: true,
            allow_active_app: false,
            allow_browser_domain: false,
            allow_session_tracking: false,
            max_related_items: 3,
            max_snippet_chars: 200,
            min_relationship_strength: 0.40,
            scrub_secrets: true,
        }
    }
}

impl AIPrivacyPolicy {
    /// Builds an AIPrivacyPolicy mapped directly from the user's persisted PrivacySettings.
    pub fn from_privacy_settings(settings: &PrivacySettings) -> Self {
        Self {
            enable_ai_reasoning: settings.enable_ai_reasoning,
            allow_active_app: settings.enable_active_app,
            allow_browser_domain: settings.enable_browser_domain,
            allow_session_tracking: settings.enable_session_tracking,
            max_related_items: 3,
            max_snippet_chars: 200,
            min_relationship_strength: 0.40,
            scrub_secrets: true,
        }
    }
}

/// Errors raised when the AI Privacy Boundary detects disallowed or unsanitized data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AIPrivacyViolation {
    AIReasoningDisabled,
    ExcessiveRelatedItems { count: usize, max: usize },
    UnrelatedItemDetected { item_id: String, reason: String },
    DisallowedApplicationMetadata { field: String },
    RawEmbeddingsDetected,
}

impl std::fmt::Display for AIPrivacyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AIReasoningDisabled => write!(
                f,
                "AI reasoning is disabled in Privacy Settings. ContextSplit continues using deterministic Context Engine logic."
            ),
            Self::ExcessiveRelatedItems { count, max } => write!(
                f,
                "Privacy violation: related items count ({}) exceeds maximum limit ({})",
                count, max
            ),
            Self::UnrelatedItemDetected { item_id, reason } => write!(
                f,
                "Privacy violation: unrelated item '{}' rejected: {}",
                item_id, reason
            ),
            Self::DisallowedApplicationMetadata { field } => write!(
                f,
                "Privacy violation: disallowed application metadata in '{}'",
                field
            ),
            Self::RawEmbeddingsDetected => write!(
                f,
                "Privacy violation: raw vector embeddings detected in evidence package"
            ),
        }
    }
}

impl std::error::Error for AIPrivacyViolation {}

/// Privacy Boundary validator and sanitizer for EvidencePackages.
#[derive(Debug, Clone)]
pub struct AIPrivacyBoundary {
    pub policy: AIPrivacyPolicy,
}

impl AIPrivacyBoundary {
    pub fn new(policy: AIPrivacyPolicy) -> Self {
        Self { policy }
    }

    pub fn from_settings(settings: &PrivacySettings) -> Self {
        Self {
            policy: AIPrivacyPolicy::from_privacy_settings(settings),
        }
    }

    pub fn from_connection(conn: &Connection) -> Result<Self, String> {
        let settings = crate::db::settings::get_settings(conn)
            .unwrap_or_default();
        Ok(Self::from_settings(&settings))
    }

    /// Verifies if remote AI reasoning is permitted under current policy.
    pub fn check_permission(&self) -> Result<(), AIPrivacyViolation> {
        if !self.policy.enable_ai_reasoning {
            return Err(AIPrivacyViolation::AIReasoningDisabled);
        }
        Ok(())
    }

    /// Audits and sanitizes an EvidencePackage in-place according to policy.
    /// Guarantees:
    /// - AI reasoning enabled check
    /// - Zero full clipboard dumps (enforces max_related_items)
    /// - Zero unrelated items (filters below min_relationship_strength)
    /// - Zero unscrubbed secrets, credentials, or tokens
    /// - Zero disallowed application metadata (app_name, domain, window title)
    /// - Zero session tracking if disabled
    /// - Zero raw vector embeddings
    pub fn audit_and_sanitize(&self, package: &mut EvidencePackage) -> Result<(), AIPrivacyViolation> {
        // 1. Master toggle enforcement
        self.check_permission()?;

        // 2. Sanitize Target Item
        if !self.policy.allow_active_app {
            package.target_item.app_name = None;
        }
        if !self.policy.allow_browser_domain {
            package.target_item.domain = None;
        }
        if !self.policy.allow_session_tracking {
            package.target_item.session_id = None;
        }
        if self.policy.scrub_secrets {
            package.target_item.content_snippet = scrub_private_data(&package.target_item.content_snippet);
            package.target_item.title = package.target_item.title.as_deref().map(scrub_private_data);
        }
        // Enforce snippet length bound
        if package.target_item.content_snippet.chars().count() > self.policy.max_snippet_chars {
            let truncated: String = package
                .target_item
                .content_snippet
                .chars()
                .take(self.policy.max_snippet_chars)
                .collect();
            package.target_item.content_snippet = format!("{}...", truncated);
        }

        // 3. Sanitize and Filter Related Items (Prevent Full Clipboard Dumps & Unrelated Items)
        // Discard any items with relationship strength below threshold
        let min_strength = self.policy.min_relationship_strength;
        package.relevant_related_items.retain(|item| item.relevance_score >= min_strength);

        // Cap at max_related_items
        if package.relevant_related_items.len() > self.policy.max_related_items {
            package.relevant_related_items.truncate(self.policy.max_related_items);
        }

        // Sanitize remaining related items
        for item in &mut package.relevant_related_items {
            if self.policy.scrub_secrets {
                item.snippet = scrub_private_data(&item.snippet);
                item.title = item.title.as_deref().map(scrub_private_data);
            }
            if item.snippet.chars().count() > self.policy.max_snippet_chars {
                let truncated: String = item
                    .snippet
                    .chars()
                    .take(self.policy.max_snippet_chars)
                    .collect();
                item.snippet = format!("{}...", truncated);
            }
        }

        // 4. Sanitize Contextual Signals
        package.contextual_signals.retain(|sig| {
            if sig.signal_type == "active_app" && !self.policy.allow_active_app {
                return false;
            }
            if sig.signal_type == "url_metadata" && !self.policy.allow_browser_domain {
                return false;
            }
            true
        });

        if self.policy.scrub_secrets {
            for sig in &mut package.contextual_signals {
                sig.summary = scrub_private_data(&sig.summary);
            }
        }

        // 5. Sanitize Session Context
        if !self.policy.allow_session_tracking {
            package.session_context = None;
            package.relevant_sessions.clear();
        } else if self.policy.scrub_secrets {
            for sess in &mut package.relevant_sessions {
                for ws in &mut sess.associated_workspaces {
                    *ws = scrub_private_data(ws);
                }
            }
            if let Some(target_sess) = &mut package.session_context {
                for ws in &mut target_sess.associated_workspaces {
                    *ws = scrub_private_data(ws);
                }
            }
        }

        // 6. Sanitize Candidate Context Names
        if self.policy.scrub_secrets {
            for cand in &mut package.candidate_contexts {
                cand.name = scrub_private_data(&cand.name);
            }
            for cand in &mut package.candidate_new_contexts {
                cand.name = scrub_private_data(&cand.name);
            }
        }

        // 7. Sanitize Semantic Summaries & Workspace Profiles (Never Send Raw Embeddings)
        for sem in &mut package.semantic_similarity_summaries {
            if self.policy.scrub_secrets {
                sem.summary = scrub_private_data(&sem.summary);
                sem.other_item_title = sem.other_item_title.as_deref().map(scrub_private_data);
                sem.workspace_name = sem.workspace_name.as_deref().map(scrub_private_data);
            }
            if sem.summary.chars().count() > self.policy.max_snippet_chars {
                let truncated: String = sem.summary.chars().take(self.policy.max_snippet_chars).collect();
                sem.summary = format!("{}...", truncated);
            }
        }

        for ws_prof in &mut package.workspace_semantic_profiles {
            if self.policy.scrub_secrets {
                ws_prof.workspace_name = scrub_private_data(&ws_prof.workspace_name);
                ws_prof.theme_summary = scrub_private_data(&ws_prof.theme_summary);
            }
        }

        // 8. Sanitize Unified ContextEvidence Records
        package.contextual_evidence.retain(|ev| {
            if !self.policy.allow_active_app
                && (ev.category == crate::context_engine::evidence::EvidenceCategory::SourceApp
                    || ev.category == crate::context_engine::evidence::EvidenceCategory::SourceWindow)
            {
                return false;
            }
            if !self.policy.allow_browser_domain
                && (ev.category == crate::context_engine::evidence::EvidenceCategory::BrowserDomain
                    || ev.category == crate::context_engine::evidence::EvidenceCategory::BrowserUrl)
            {
                return false;
            }
            if !self.policy.allow_session_tracking
                && ev.category == crate::context_engine::evidence::EvidenceCategory::SameSession
            {
                return false;
            }
            true
        });

        if self.policy.scrub_secrets {
            for ev in &mut package.contextual_evidence {
                ev.explanation.title = scrub_private_data(&ev.explanation.title);
                ev.explanation.description = scrub_private_data(&ev.explanation.description);
                if let Some(badge) = &mut ev.explanation.badge {
                    *badge = scrub_private_data(badge);
                }
                for val in ev.attributes.values_mut() {
                    if let Some(s) = val.as_str() {
                        *val = serde_json::Value::String(scrub_private_data(s));
                    }
                }
            }
        }

        // 9. Record Privacy Boundary Stamp
        package.system_metadata.privacy_boundary = "ai_privacy_boundary:strict_curated_evidence_only:v1".to_string();

        Ok(())
    }
}

/// Comprehensive scrubber for secrets, credentials, auth tokens, passwords, and connection strings.
pub fn scrub_private_data(input: &str) -> String {
    let mut safe = input.to_string();

    // 1. PEM Private Keys
    let pem_re = Regex::new(r"(?s)-----BEGIN [A-Z\s]+PRIVATE KEY-----.*?-----END [A-Z\s]+PRIVATE KEY-----").unwrap();
    safe = pem_re.replace_all(&safe, "[PRIVATE_KEY_REDACTED]").to_string();

    // 2. Database Connection Strings (Postgres, MySQL, Mongo, Redis, AMQP, etc.)
    let conn_str_re = Regex::new(r"(?i)\b(postgres|postgresql|mysql|mongodb|redis|amqp|mssql)://[^\s<>]+").unwrap();
    safe = conn_str_re.replace_all(&safe, "[CONNECTION_STRING_REDACTED]").to_string();

    // 3. URLs with Basic Auth credentials (https://user:password@host)
    let basic_auth_re = Regex::new(r"https?://[a-zA-Z0-9_.\-]+:[^@\s]+@[^\s<>]+").unwrap();
    safe = basic_auth_re.replace_all(&safe, "[CREDENTIAL_REDACTED]").to_string();

    // 4. Specific Token Formats (JWT, AWS, OpenAI, GitHub, GitLab, Slack, Google)
    let token_patterns = [
        // Bearer tokens
        (Regex::new(r"Bearer\s+[A-Za-z0-9\-\._~+/]+=*").unwrap(), "Bearer [REDACTED]"),
        // JWT tokens (three dot-separated base64url segments)
        (Regex::new(r"ey[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+").unwrap(), "[JWT_REDACTED]"),
        // AWS Access Key ID (AKIA..., ASIA...)
        (Regex::new(r"\b(AKIA|ABIA|ACCA|ASIA)[0-9A-Z]{16}\b").unwrap(), "[AWS_KEY_REDACTED]"),
        // OpenAI API Key
        (Regex::new(r"sk-[A-Za-z0-9_-]{20,}").unwrap(), "[API_KEY_REDACTED]"),
        // GitHub Token (ghp_, gho_, ghu_, ghs_, ghr_)
        (Regex::new(r"(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}").unwrap(), "[API_KEY_REDACTED]"),
        // GitLab Personal Access Token
        (Regex::new(r"glpat-[A-Za-z0-9_-]{20,}").unwrap(), "[API_KEY_REDACTED]"),
        // Slack Token
        (Regex::new(r"xox[baprs]-[A-Za-z0-9_-]{10,}").unwrap(), "[API_KEY_REDACTED]"),
        // Google API Key
        (Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap(), "[API_KEY_REDACTED]"),
        // Credit card numbers
        (Regex::new(r"\b(?:\d[ -]*?){13,16}\b").unwrap(), "[CARD_REDACTED]"),
    ];

    for (re, replacement) in &token_patterns {
        safe = re.replace_all(&safe, *replacement).to_string();
    }

    // 5. Generic credential assignments (password=..., token: ..., secret = ...) on same line
    let generic_cred_re = Regex::new(
        r#"(?i)\b(password|passwd|secret|apikey|api_key|token|auth_token|access_token|private_key|aws_secret_access_key)[ \t]*[:=][ \t]*["']?[^ \t\r\n,;"'\[\]]+"#,
    ).unwrap();
    safe = generic_cred_re.replace_all(&safe, "$1: [REDACTED]").to_string();

    safe
}
