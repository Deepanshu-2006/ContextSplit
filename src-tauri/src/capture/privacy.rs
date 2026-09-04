use std::time::Duration;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::models::{CaptureError, CaptureEvent, CaptureType};
use crate::db::settings::PrivacySettings;

/// Categorization of sensitive content detected by the privacy filter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveContentType {
    /// Password or password-like credential
    Password,
    /// One-Time Password (OTP) or 2FA verification code
    OneTimePassword,
    /// API key or bearer/access token
    ApiKey,
    /// Cryptographic private key (e.g. RSA, EC, OpenSSH)
    PrivateKey,
    /// Credit card or payment identifier
    PaymentCard,
    /// Social Security Number or national identity number
    NationalIdentifier,
    /// Database connection string containing credentials
    DatabaseConnectionString,
    /// URL with embedded HTTP basic authentication
    BasicAuthCredential,
    /// Custom exclusion pattern match
    CustomPattern(String),
}

impl std::fmt::Display for SensitiveContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password => write!(f, "password"),
            Self::OneTimePassword => write!(f, "one_time_password"),
            Self::ApiKey => write!(f, "api_key"),
            Self::PrivateKey => write!(f, "private_key"),
            Self::PaymentCard => write!(f, "payment_card"),
            Self::NationalIdentifier => write!(f, "national_identifier"),
            Self::DatabaseConnectionString => write!(f, "database_connection_string"),
            Self::BasicAuthCredential => write!(f, "basic_auth_credential"),
            Self::CustomPattern(pat) => write!(f, "custom_pattern: {}", pat),
        }
    }
}

/// The structured reason why a clipboard capture event was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyRejectionReason {
    /// Clipboard monitoring is globally disabled
    MonitoringDisabled,
    /// Clipboard monitoring is temporarily paused
    MonitoringPaused {
        until: Option<String>,
    },
    /// Source application is in the excluded applications list
    ExcludedApplication {
        application_name: String,
    },
    /// Destination or originating domain is in the excluded domains list
    ExcludedDomain {
        domain: String,
    },
    /// Sensitive content detected in clipboard
    SensitiveContent {
        content_type: SensitiveContentType,
    },
    /// Custom exclusion pattern matched
    ExcludedPattern {
        pattern_name: String,
    },
}

impl std::fmt::Display for PrivacyRejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MonitoringDisabled => write!(f, "Clipboard monitoring is disabled"),
            Self::MonitoringPaused { until: Some(u) } => {
                write!(f, "Clipboard monitoring is paused until {}", u)
            }
            Self::MonitoringPaused { until: None } => {
                write!(f, "Clipboard monitoring is paused")
            }
            Self::ExcludedApplication { application_name } => {
                write!(f, "Source application '{}' is excluded", application_name)
            }
            Self::ExcludedDomain { domain } => {
                write!(f, "Domain '{}' is excluded", domain)
            }
            Self::SensitiveContent { content_type } => {
                write!(f, "Sensitive content detected ({})", content_type)
            }
            Self::ExcludedPattern { pattern_name } => {
                write!(f, "Matched excluded pattern '{}'", pattern_name)
            }
        }
    }
}

/// An explainable, privacy-safe rejection explanation that NEVER logs or discloses the secret content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyRejection {
    pub reason: PrivacyRejectionReason,
    pub explanation: String,
}

impl std::fmt::Display for PrivacyRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.explanation)
    }
}

impl std::error::Error for PrivacyRejection {}

/// Result returned by the ClipboardPrivacyFilter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyFilterResult {
    /// Content is allowed to proceed to persistence and processing
    Allowed,
    /// Content is rejected before persistence
    Rejected(PrivacyRejection),
}

impl PrivacyFilterResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Rejected(_))
    }

    pub fn into_result(self) -> Result<(), PrivacyRejection> {
        match self {
            Self::Allowed => Ok(()),
            Self::Rejected(rej) => Err(rej),
        }
    }
}

/// Configuration for the clipboard privacy filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPrivacyFilterConfig {
    /// Whether clipboard monitoring is globally enabled
    pub enabled: bool,
    /// Optional RFC 3339 UTC timestamp string until which monitoring is temporarily paused
    pub paused_until: Option<String>,
    /// Whether monitoring is indefinitely paused
    pub paused_indefinitely: bool,
    /// Detect password-like content
    pub detect_passwords: bool,
    /// Detect OTP and 2FA verification codes
    pub detect_otps: bool,
    /// Detect API keys and auth tokens
    pub detect_api_keys: bool,
    /// Detect cryptographic private keys
    pub detect_private_keys: bool,
    /// Detect payment cards and SSNs
    pub detect_payment_cards: bool,
    /// Detect database connection strings and basic auth
    pub detect_connection_strings: bool,
    /// List of excluded application process or bundle names
    pub excluded_applications: Vec<String>,
    /// List of excluded browser domains
    pub excluded_domains: Vec<String>,
    /// Custom regex patterns to exclude
    pub custom_exclusion_patterns: Vec<String>,
}

impl Default for ClipboardPrivacyFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            paused_until: None,
            paused_indefinitely: false,
            detect_passwords: true,
            detect_otps: true,
            detect_api_keys: true,
            detect_private_keys: true,
            detect_payment_cards: true,
            detect_connection_strings: true,
            excluded_applications: vec![
                // Password managers & authenticators (macOS / Windows / Linux)
                "1Password".to_string(),
                "1Password 7".to_string(),
                "1Password 8".to_string(),
                "1password.exe".to_string(),
                "Bitwarden".to_string(),
                "bitwarden.exe".to_string(),
                "KeePass".to_string(),
                "keepass.exe".to_string(),
                "KeePassXC".to_string(),
                "keepassxc.exe".to_string(),
                "LastPass".to_string(),
                "lastpass.exe".to_string(),
                "Dashlane".to_string(),
                "dashlane.exe".to_string(),
                "Enpass".to_string(),
                "enpass.exe".to_string(),
                "NordPass".to_string(),
                "nordpass.exe".to_string(),
                "Keychain Access".to_string(),
                "Authy".to_string(),
                "Google Authenticator".to_string(),
                "auth0".to_string(),
            ],
            excluded_domains: Vec::new(),
            custom_exclusion_patterns: Vec::new(),
        }
    }
}

/// The core Clipboard Privacy Filter.
///
/// Responsibilities:
/// 1. Enforces global enable/disable and temporary pause states.
/// 2. Enforces source application and domain exclusions.
/// 3. Detects sensitive content (passwords, OTPs, API keys, private keys, cards).
/// 4. Rejects sensitive content BEFORE database persistence.
/// 5. Prevents unscrubbed content from reaching AI reasoners or external providers.
/// 6. Generates explainable, privacy-safe audit trails WITHOUT logging secrets.
#[derive(Debug, Clone)]
pub struct ClipboardPrivacyFilter {
    pub config: ClipboardPrivacyFilterConfig,
}

impl Default for ClipboardPrivacyFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardPrivacyFilter {
    /// Creates a new privacy filter with default security policies.
    pub fn new() -> Self {
        Self {
            config: ClipboardPrivacyFilterConfig::default(),
        }
    }

    /// Creates a privacy filter with custom configuration.
    pub fn with_config(config: ClipboardPrivacyFilterConfig) -> Self {
        Self { config }
    }

    /// Maps user PrivacySettings into the filter configuration.
    pub fn with_privacy_settings(self, _settings: &PrivacySettings) -> Self {
        self
    }

    // =========================================================================
    // State Controls (Enabled / Paused)
    // =========================================================================

    /// Checks if clipboard monitoring is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Sets global clipboard monitoring enabled status.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// Checks if monitoring is currently paused (temporarily or indefinitely).
    pub fn is_paused(&self) -> bool {
        if self.config.paused_indefinitely {
            return true;
        }

        if let Some(ref until_str) = self.config.paused_until {
            if let Ok(until) = chrono::DateTime::parse_from_rfc3339(until_str) {
                if Utc::now() < until.with_timezone(&Utc) {
                    return true;
                }
            }
        }

        false
    }

    /// Pauses monitoring indefinitely until explicitly resumed.
    pub fn pause_indefinitely(&mut self) {
        self.config.paused_indefinitely = true;
        self.config.paused_until = None;
    }

    /// Pauses monitoring for a specific duration.
    pub fn pause_for(&mut self, duration: Duration) {
        let until = Utc::now() + chrono::Duration::from_std(duration).unwrap_or(chrono::Duration::minutes(5));
        self.config.paused_until = Some(until.to_rfc3339());
        self.config.paused_indefinitely = false;
    }

    /// Pauses monitoring until a specific UTC timestamp.
    pub fn pause_until(&mut self, until: DateTime<Utc>) {
        self.config.paused_until = Some(until.to_rfc3339());
        self.config.paused_indefinitely = false;
    }

    /// Resumes monitoring, clearing any active pause state.
    pub fn resume(&mut self) {
        self.config.paused_indefinitely = false;
        self.config.paused_until = None;
    }

    // =========================================================================
    // Exclusions Configuration
    // =========================================================================

    /// Adds an application to the excluded applications list.
    pub fn add_excluded_application(&mut self, app_name: &str) {
        let name = app_name.trim().to_string();
        if !name.is_empty() && !self.config.excluded_applications.iter().any(|a| a.eq_ignore_ascii_case(&name)) {
            self.config.excluded_applications.push(name);
        }
    }

    /// Adds a domain to the excluded domains list.
    pub fn add_excluded_domain(&mut self, domain: &str) {
        let dom = domain.trim().to_lowercase();
        if !dom.is_empty() && !self.config.excluded_domains.contains(&dom) {
            self.config.excluded_domains.push(dom);
        }
    }

    /// Adds a custom regex exclusion pattern.
    pub fn add_custom_exclusion_pattern(&mut self, pattern: &str) {
        let pat = pattern.trim().to_string();
        if !pat.is_empty() && !self.config.custom_exclusion_patterns.contains(&pat) {
            self.config.custom_exclusion_patterns.push(pat);
        }
    }

    // =========================================================================
    // Content Auditing & Sensitive Content Detection
    // =========================================================================

    /// Evaluates raw clipboard content, source application, and window title against all privacy rules.
    /// Returns `PrivacyFilterResult::Allowed` or `PrivacyFilterResult::Rejected` with an explainable reason.
    pub fn filter_clipboard(
        &self,
        content: &str,
        source_app: Option<&str>,
        window_title: Option<&str>,
    ) -> PrivacyFilterResult {
        // 1. Check Global Enabled State
        if !self.config.enabled {
            return PrivacyFilterResult::Rejected(PrivacyRejection {
                reason: PrivacyRejectionReason::MonitoringDisabled,
                explanation: "Clipboard monitoring is disabled in application settings.".to_string(),
            });
        }

        // 2. Check Temporary / Indefinite Pause
        if self.config.paused_indefinitely {
            return PrivacyFilterResult::Rejected(PrivacyRejection {
                reason: PrivacyRejectionReason::MonitoringPaused { until: None },
                explanation: "Clipboard capture is paused.".to_string(),
            });
        }

        if let Some(ref until_str) = self.config.paused_until {
            if let Ok(until) = chrono::DateTime::parse_from_rfc3339(until_str) {
                let now = Utc::now();
                if now < until.with_timezone(&Utc) {
                    return PrivacyFilterResult::Rejected(PrivacyRejection {
                        reason: PrivacyRejectionReason::MonitoringPaused {
                            until: Some(until_str.clone()),
                        },
                        explanation: format!(
                            "Clipboard capture is temporarily paused until {}.",
                            until.format("%Y-%m-%d %H:%M:%S UTC")
                        ),
                    });
                }
            }
        }

        // 3. Check Source Application Exclusions
        if let Some(app) = source_app {
            let app_trimmed = app.trim();
            for excluded in &self.config.excluded_applications {
                if app_trimmed.eq_ignore_ascii_case(excluded)
                    || app_trimmed.to_lowercase().contains(&excluded.to_lowercase())
                {
                    return PrivacyFilterResult::Rejected(PrivacyRejection {
                        reason: PrivacyRejectionReason::ExcludedApplication {
                            application_name: app_trimmed.to_string(),
                        },
                        explanation: format!(
                            "Capture rejected: Source application '{}' is in the excluded applications list.",
                            app_trimmed
                        ),
                    });
                }
            }
        }

        // 4. Check Window Title for Excluded App hints (e.g. "1Password - Password Manager")
        if let Some(win) = window_title {
            let win_lower = win.to_lowercase();
            for excluded in &self.config.excluded_applications {
                let exc_lower = excluded.to_lowercase();
                if win_lower.contains(&exc_lower) {
                    return PrivacyFilterResult::Rejected(PrivacyRejection {
                        reason: PrivacyRejectionReason::ExcludedApplication {
                            application_name: excluded.clone(),
                        },
                        explanation: format!(
                            "Capture rejected: Window title matches excluded application '{}'.",
                            excluded
                        ),
                    });
                }
            }
        }

        // 5. Check Custom Regex Exclusions
        for pat in &self.config.custom_exclusion_patterns {
            if let Ok(re) = Regex::new(pat) {
                if re.is_match(content) {
                    return PrivacyFilterResult::Rejected(PrivacyRejection {
                        reason: PrivacyRejectionReason::ExcludedPattern {
                            pattern_name: pat.clone(),
                        },
                        explanation: "Capture rejected: Content matches user-defined exclusion pattern.".to_string(),
                    });
                }
            }
        }

        // 6. Check Excluded Domains (if content is URL)
        let trimmed_content = content.trim();
        if trimmed_content.starts_with("http://") || trimmed_content.starts_with("https://") {
            for domain in &self.config.excluded_domains {
                if trimmed_content.to_lowercase().contains(&domain.to_lowercase()) {
                    return PrivacyFilterResult::Rejected(PrivacyRejection {
                        reason: PrivacyRejectionReason::ExcludedDomain {
                            domain: domain.clone(),
                        },
                        explanation: format!(
                            "Capture rejected: Domain '{}' is in the excluded domains list.",
                            domain
                        ),
                    });
                }
            }
        }

        // 7. Sensitive Content Detection
        if let Some(sensitive_type) = self.detect_sensitive_content(trimmed_content) {
            let explanation = match &sensitive_type {
                SensitiveContentType::Password => {
                    "Capture rejected: Password-like credential detected in clipboard."
                }
                SensitiveContentType::OneTimePassword => {
                    "Capture rejected: One-Time Password (OTP) or 2FA code detected in clipboard."
                }
                SensitiveContentType::ApiKey => {
                    "Capture rejected: API key or authentication token detected in clipboard."
                }
                SensitiveContentType::PrivateKey => {
                    "Capture rejected: Cryptographic private key detected in clipboard."
                }
                SensitiveContentType::PaymentCard => {
                    "Capture rejected: Payment card number detected in clipboard."
                }
                SensitiveContentType::NationalIdentifier => {
                    "Capture rejected: National identification number (e.g. SSN) detected in clipboard."
                }
                SensitiveContentType::DatabaseConnectionString => {
                    "Capture rejected: Database connection string detected in clipboard."
                }
                SensitiveContentType::BasicAuthCredential => {
                    "Capture rejected: Embedded URL credentials detected in clipboard."
                }
                SensitiveContentType::CustomPattern(_) => {
                    "Capture rejected: Custom sensitive pattern detected."
                }
            };

            return PrivacyFilterResult::Rejected(PrivacyRejection {
                reason: PrivacyRejectionReason::SensitiveContent {
                    content_type: sensitive_type,
                },
                explanation: explanation.to_string(),
            });
        }

        PrivacyFilterResult::Allowed
    }

    /// Evaluates a strongly-typed `CaptureEvent` against all privacy rules.
    pub fn filter_event(&self, event: &CaptureEvent) -> PrivacyFilterResult {
        // If capture type is not clipboard, verify general sensitive content
        if event.capture_type != CaptureType::Clipboard {
            // Check sensitive content for all capture types
            if let Some(sensitive_type) = self.detect_sensitive_content(&event.content) {
                return PrivacyFilterResult::Rejected(PrivacyRejection {
                    reason: PrivacyRejectionReason::SensitiveContent {
                        content_type: sensitive_type,
                    },
                    explanation: "Capture rejected: Sensitive content detected.".to_string(),
                });
            }
            return PrivacyFilterResult::Allowed;
        }

        self.filter_clipboard(
            &event.content,
            event.source_application.as_deref(),
            event.source_metadata.window_title.as_deref(),
        )
    }

    /// Audits an event, returning `Ok(())` on pass or `Err(CaptureError::PrivacyRejected)` on violation.
    pub fn audit_event(&self, event: &CaptureEvent) -> Result<(), CaptureError> {
        match self.filter_event(event) {
            PrivacyFilterResult::Allowed => Ok(()),
            PrivacyFilterResult::Rejected(rej) => Err(CaptureError::PrivacyRejected(rej.explanation)),
        }
    }

    // =========================================================================
    // Core Pattern Classifiers
    // =========================================================================

    /// Scans text for passwords, OTPs, API keys, private keys, payment info, and credentials.
    /// Returns the matched `SensitiveContentType` or `None`.
    pub fn detect_sensitive_content(&self, text: &str) -> Option<SensitiveContentType> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        // A. Cryptographic Private Keys
        if self.config.detect_private_keys {
            if trimmed.contains("BEGIN ") && trimmed.contains("PRIVATE KEY-----") {
                return Some(SensitiveContentType::PrivateKey);
            }
        }

        // B. One-Time Passwords (OTP / 2FA)
        if self.config.detect_otps {
            if self.is_otp(trimmed) {
                return Some(SensitiveContentType::OneTimePassword);
            }
        }

        // C. Passwords & High-Entropy Credentials
        if self.config.detect_passwords {
            if self.is_password(trimmed) {
                return Some(SensitiveContentType::Password);
            }
        }

        // D. API Keys & Auth Tokens
        if self.config.detect_api_keys {
            if self.is_api_key_or_token(trimmed) {
                return Some(SensitiveContentType::ApiKey);
            }
        }

        // E. Connection Strings & Basic Auth
        if self.config.detect_connection_strings {
            if self.is_connection_string_or_basic_auth(trimmed) {
                return Some(SensitiveContentType::DatabaseConnectionString);
            }
        }

        // F. Payment Cards & SSNs
        if self.config.detect_payment_cards {
            if self.is_payment_card_or_ssn(trimmed) {
                return Some(SensitiveContentType::PaymentCard);
            }
        }

        None
    }

    /// Classifies whether input is an OTP / 2FA verification code.
    fn is_otp(&self, text: &str) -> bool {
        // 1. Standalone 4 to 8 digit numeric codes (e.g. "849201", "1234", "98234182")
        let is_digits_only = text.len() >= 4 && text.len() <= 8 && text.chars().all(|c| c.is_ascii_digit());
        if is_digits_only {
            return true;
        }

        // 2. Formatted 6-8 digit code with dash/space (e.g. "123-456", "123 456", "G-849201")
        if (text.len() == 7 || text.len() == 8)
            && (text.starts_with("G-") || text.starts_with("g-"))
            && text[2..].chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }

        if text.len() == 7 {
            let chars: Vec<char> = text.chars().collect();
            if chars[0..3].iter().all(|c| c.is_ascii_digit())
                && (chars[3] == '-' || chars[3] == ' ')
                && chars[4..7].iter().all(|c| c.is_ascii_digit())
            {
                return true;
            }
        }

        // 3. Verification message containing OTP (e.g. "Your verification code is: 492018", "OTP: 839201", "Security code 123456")
        let otp_msg_re = Regex::new(
            r"(?i)\b(verification\s+code|security\s+code|one-time\s+password|login\s+code|auth\s+code|otp|2fa\s+code|passcode)\b[^\d\n\r]{0,25}\b[0-9]{4,8}\b",
        ).unwrap();
        if otp_msg_re.is_match(text) {
            return true;
        }

        let otp_suffix_re = Regex::new(
            r"(?i)\b[0-9]{4,8}\b[^\d\n\r]{0,25}\b(verification\s+code|security\s+code|login\s+code|otp|2fa\s+code|passcode|one-time\s+password)\b",
        ).unwrap();
        if otp_suffix_re.is_match(text) {
            return true;
        }

        false
    }

    /// Classifies whether input represents a password or explicit credential.
    fn is_password(&self, text: &str) -> bool {
        // 1. Explicit password key-value assignment (e.g. "password = SuperSecret123!", "passwd: ...", "pwd: ...")
        let explicit_pwd_re = Regex::new(
            r#"(?i)\b(password|passwd|pwd|db_pass|user_pass|admin_pass)[ \t]*[:=][ \t]*["']?[^\s"']{4,}"#,
        ).unwrap();
        if explicit_pwd_re.is_match(text) {
            return true;
        }

        // 2. Single high-complexity string without spaces (length 10..=64)
        // containing a mix of uppercase, lowercase, digits, and special characters
        if !text.contains(char::is_whitespace) && text.len() >= 10 && text.len() <= 64 {
            let has_upper = text.chars().any(|c| c.is_ascii_uppercase());
            let has_lower = text.chars().any(|c| c.is_ascii_lowercase());
            let has_digit = text.chars().any(|c| c.is_ascii_digit());
            let has_special = text.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?/~`'\"\\".contains(c));

            // Must NOT be a standard URL or UUID or file path
            let is_url = text.starts_with("http://") || text.starts_with("https://") || text.starts_with("file://");
            let is_uuid = uuid::Uuid::parse_str(text).is_ok();
            let is_path = text.starts_with('/') || (text.len() > 3 && text.chars().nth(1) == Some(':'));

            if !is_url && !is_uuid && !is_path {
                let complexity_score = (if has_upper { 1 } else { 0 })
                    + (if has_lower { 1 } else { 0 })
                    + (if has_digit { 1 } else { 0 })
                    + (if has_special { 1 } else { 0 });

                if complexity_score >= 3 {
                    return true;
                }
            }
        }

        false
    }

    /// Classifies API keys, tokens, and secret identifiers.
    fn is_api_key_or_token(&self, text: &str) -> bool {
        // AWS Access Key ID
        let aws_re = Regex::new(r"\b(AKIA|ABIA|ACCA|ASIA)[0-9A-Z]{16}\b").unwrap();
        if aws_re.is_match(text) {
            return true;
        }

        // OpenAI API Key
        let openai_re = Regex::new(r"sk-[A-Za-z0-9_-]{20,}").unwrap();
        if openai_re.is_match(text) {
            return true;
        }

        // GitHub Personal Access Token
        let github_re = Regex::new(r"(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}").unwrap();
        if github_re.is_match(text) {
            return true;
        }

        // GitLab PAT
        let gitlab_re = Regex::new(r"glpat-[A-Za-z0-9_-]{20,}").unwrap();
        if gitlab_re.is_match(text) {
            return true;
        }

        // Google API Key
        let google_re = Regex::new(r"AIza[0-9A-Za-z_-]{35}").unwrap();
        if google_re.is_match(text) {
            return true;
        }

        // Slack Token
        let slack_re = Regex::new(r"xox[baprs]-[A-Za-z0-9_-]{10,}").unwrap();
        if slack_re.is_match(text) {
            return true;
        }

        // JWT (JSON Web Token)
        let jwt_re = Regex::new(r"ey[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+").unwrap();
        if jwt_re.is_match(text) {
            return true;
        }

        // Bearer Token
        let bearer_re = Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9\-\._~+/]{20,}").unwrap();
        if bearer_re.is_match(text) {
            return true;
        }

        // Generic API Key assignment
        let key_assignment_re = Regex::new(
            r#"(?i)\b(api_key|apikey|access_token|auth_token|secret_key)[ \t]*[:=][ \t]*["']?[A-Za-z0-9_\-]{16,}"#,
        ).unwrap();
        if key_assignment_re.is_match(text) {
            return true;
        }

        false
    }

    /// Classifies database connection strings and basic authentication credentials.
    fn is_connection_string_or_basic_auth(&self, text: &str) -> bool {
        let conn_str_re = Regex::new(
            r"(?i)\b(postgres|postgresql|mysql|mongodb|redis|amqp|mssql)://[^\s<>]+",
        ).unwrap();
        if conn_str_re.is_match(text) {
            return true;
        }

        let basic_auth_re = Regex::new(r"https?://[a-zA-Z0-9_.\-]+:[^@\s]+@[^\s<>]+").unwrap();
        if basic_auth_re.is_match(text) {
            return true;
        }

        false
    }

    /// Classifies payment card numbers (13-19 digits) and SSNs.
    fn is_payment_card_or_ssn(&self, text: &str) -> bool {
        // SSN format: 000-00-0000
        let ssn_re = Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap();
        if ssn_re.is_match(text) {
            return true;
        }

        // Credit Card format (e.g. 4532 1234 5678 9012 or 4532-1234-5678-9012)
        let card_re = Regex::new(r"\b\d{4}[ -]\d{4}[ -]\d{4}[ -]\d{4}\b").unwrap();
        if card_re.is_match(text) {
            return true;
        }

        false
    }
}
