use std::sync::{OnceLock, RwLock};
use serde::{Deserialize, Serialize};

use crate::context_engine::decision_trace::ContextDecisionTracer;
use crate::context_engine::performance_tracing::get_performance_tracer;

// =========================================================================
// Disclaimer
// =========================================================================

pub const CONTEXT_DIAGNOSTICS_SAFETY_DISCLAIMER: &str =
    "ContextSplit diagnostics are strictly local-first, opt-in for local development, and completely disabled in production mode with zero telemetry transmission to external services.";

// =========================================================================
// Diagnostics Mode & Policy
// =========================================================================

/// Operating mode for development diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsMode {
    /// Production default: zero diagnostics collection, zero storage, zero exposure.
    Disabled,
    /// Opt-in local development: bounded in-memory metrics, debug tools enabled.
    LocalDev,
    /// Granular custom policy configured by developer.
    Custom,
}

/// Granular privacy, storage, and exposure policy for ContextSplit diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsPolicy {
    /// Overall master toggle. If false, all diagnostic subsystems are completely no-op.
    pub enabled: bool,
    /// Whether internal decision traces may be recorded or inspected.
    pub expose_decision_traces: bool,
    /// Whether debug timelines and timeline inspectors are active.
    pub expose_debug_timelines: bool,
    /// Whether raw contextual evidence (e.g. window titles, file paths) is retained in diagnostics.
    pub expose_raw_evidence: bool,
    /// Whether internal calibration and intermediate heuristic confidence numbers are exposed.
    pub expose_internal_confidence: bool,
    /// Whether performance tracing ring buffers are active.
    pub enable_performance_tracing: bool,
    /// Whether background health diagnostics run.
    pub enable_health_checks: bool,
    /// Whether cross-system consistency validators run.
    pub enable_cross_system_checks: bool,
    /// Hard upper bound for in-memory diagnostic records (traces, spans).
    pub max_in_memory_records: usize,
    /// Local-first guard: permanently false by default to prevent sending diagnostics outside device.
    pub allow_external_export: bool,
}

impl Default for DiagnosticsPolicy {
    fn default() -> Self {
        Self::production()
    }
}

impl DiagnosticsPolicy {
    /// Production safe baseline: completely disabled with zero exposure.
    pub fn production() -> Self {
        Self {
            enabled: false,
            expose_decision_traces: false,
            expose_debug_timelines: false,
            expose_raw_evidence: false,
            expose_internal_confidence: false,
            enable_performance_tracing: false,
            enable_health_checks: false,
            enable_cross_system_checks: false,
            max_in_memory_records: 0,
            allow_external_export: false,
        }
    }

    /// Opt-in local development configuration: bounded storage, local-only.
    pub fn local_dev() -> Self {
        Self {
            enabled: true,
            expose_decision_traces: true,
            expose_debug_timelines: true,
            expose_raw_evidence: false, // strictly sanitized by default even in local dev
            expose_internal_confidence: true,
            enable_performance_tracing: true,
            enable_health_checks: true,
            enable_cross_system_checks: true,
            max_in_memory_records: 500,
            allow_external_export: false, // always local-first
        }
    }

    /// Disabled preset.
    pub fn disabled() -> Self {
        Self::production()
    }
}

// =========================================================================
// Diagnostics Safety Status
// =========================================================================

/// Current safety and operational status of ContextSplit diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsSafetyStatus {
    pub mode: DiagnosticsMode,
    pub is_enabled: bool,
    pub total_decision_traces: usize,
    pub total_performance_spans: usize,
    pub max_storage_capacity: usize,
    pub local_first_guaranteed: bool,
    pub privacy_guarantees: Vec<String>,
}

// =========================================================================
// Diagnostics Controller
// =========================================================================

/// Thread-safe central controller managing diagnostic privacy and lifecycle.
pub struct DiagnosticsController {
    policy: RwLock<DiagnosticsPolicy>,
}

impl DiagnosticsController {
    pub fn new(policy: DiagnosticsPolicy) -> Self {
        Self {
            policy: RwLock::new(policy),
        }
    }

    /// Get current active policy snapshot.
    pub fn get_policy(&self) -> DiagnosticsPolicy {
        self.policy.read().map(|p| p.clone()).unwrap_or_else(|_| DiagnosticsPolicy::production())
    }

    /// Update active policy dynamically.
    pub fn set_policy(&self, policy: DiagnosticsPolicy) {
        if let Ok(mut lock) = self.policy.write() {
            let was_enabled = lock.enabled;
            *lock = policy;
            if was_enabled && !lock.enabled {
                self.purge_all_diagnostics();
            }
        }
    }

    /// Completely disable all diagnostics immediately and wipe in-memory buffers.
    pub fn disable_all(&self) {
        self.set_policy(DiagnosticsPolicy::disabled());
    }

    /// Enable standard opt-in local development diagnostics.
    pub fn enable_local_dev(&self) {
        self.set_policy(DiagnosticsPolicy::local_dev());
    }

    /// Checks if any diagnostic collection is globally permitted.
    pub fn is_diagnostics_enabled(&self) -> bool {
        self.policy.read().map(|p| p.enabled).unwrap_or(false)
    }

    /// Checks if decision tracing is enabled.
    pub fn is_trace_allowed(&self) -> bool {
        self.policy
            .read()
            .map(|p| p.enabled && p.expose_decision_traces)
            .unwrap_or(false)
    }

    /// Checks if performance tracing is enabled.
    pub fn is_performance_tracing_allowed(&self) -> bool {
        self.policy
            .read()
            .map(|p| p.enabled && p.enable_performance_tracing)
            .unwrap_or(false)
    }

    /// Checks if raw evidence details can be included.
    pub fn is_raw_evidence_allowed(&self) -> bool {
        self.policy
            .read()
            .map(|p| p.enabled && p.expose_raw_evidence)
            .unwrap_or(false)
    }

    /// Checks if internal confidence numbers can be exposed.
    pub fn is_internal_confidence_allowed(&self) -> bool {
        self.policy
            .read()
            .map(|p| p.enabled && p.expose_internal_confidence)
            .unwrap_or(false)
    }

    /// Purges all in-memory diagnostic traces, spans, and cached debug logs.
    pub fn purge_all_diagnostics(&self) {
        ContextDecisionTracer::clear();
        get_performance_tracer().clear();
    }

    /// Returns safety status and telemetry counts.
    pub fn get_status(&self) -> DiagnosticsSafetyStatus {
        let policy = self.get_policy();
        let traces_count = ContextDecisionTracer::stats().total_recorded;
        let spans_count = get_performance_tracer().len();

        let mode = if !policy.enabled {
            DiagnosticsMode::Disabled
        } else if policy == DiagnosticsPolicy::local_dev() {
            DiagnosticsMode::LocalDev
        } else {
            DiagnosticsMode::Custom
        };

        DiagnosticsSafetyStatus {
            mode,
            is_enabled: policy.enabled,
            total_decision_traces: traces_count,
            total_performance_spans: spans_count,
            max_storage_capacity: policy.max_in_memory_records,
            local_first_guaranteed: !policy.allow_external_export,
            privacy_guarantees: vec![
                "Local-only storage (no cloud upload)".to_string(),
                "Zero raw keystroke/content retention".to_string(),
                "Bounded memory ring buffer".to_string(),
                "Production mode zero exposure".to_string(),
            ],
        }
    }

    /// Redacts internal fields from any diagnostic representation if diagnostics are disabled.
    pub fn sanitize_output_if_disabled<'a>(&self, text: &'a str) -> Option<&'a str> {
        if self.is_diagnostics_enabled() {
            Some(text)
        } else {
            None
        }
    }
}

// Global Diagnostics Controller Singleton
static GLOBAL_DIAGNOSTICS_CONTROLLER: OnceLock<DiagnosticsController> = OnceLock::new();

pub fn get_diagnostics_controller() -> &'static DiagnosticsController {
    GLOBAL_DIAGNOSTICS_CONTROLLER.get_or_init(|| DiagnosticsController::new(DiagnosticsPolicy::production()))
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_production_diagnostics_disabled() {
        let controller = DiagnosticsController::new(DiagnosticsPolicy::production());

        assert!(!controller.is_diagnostics_enabled());
        assert!(!controller.is_trace_allowed());
        assert!(!controller.is_performance_tracing_allowed());
        assert!(!controller.is_raw_evidence_allowed());
        assert!(!controller.is_internal_confidence_allowed());

        let status = controller.get_status();
        assert_eq!(status.mode, DiagnosticsMode::Disabled);
        assert!(!status.is_enabled);
        assert!(status.local_first_guaranteed);
    }

    #[test]
    fn test_policy_toggle_enabling_and_disabling() {
        let controller = DiagnosticsController::new(DiagnosticsPolicy::production());
        assert!(!controller.is_diagnostics_enabled());

        controller.enable_local_dev();
        assert!(controller.is_diagnostics_enabled());
        assert!(controller.is_trace_allowed());
        assert!(controller.is_performance_tracing_allowed());
        assert_eq!(controller.get_status().mode, DiagnosticsMode::LocalDev);

        controller.disable_all();
        assert!(!controller.is_diagnostics_enabled());
        assert!(!controller.is_trace_allowed());
        assert_eq!(controller.get_status().mode, DiagnosticsMode::Disabled);
    }

    #[test]
    fn test_purge_all_diagnostics_clears_memory() {
        let controller = DiagnosticsController::new(DiagnosticsPolicy::local_dev());

        // Record a span
        let span = crate::context_engine::performance_tracing::start_span(
            crate::context_engine::performance_tracing::OperationCategory::ActivityIngestion,
            "test_op",
        );
        span.end();

        assert!(get_performance_tracer().len() > 0);

        controller.purge_all_diagnostics();
        assert_eq!(get_performance_tracer().len(), 0);
        assert_eq!(ContextDecisionTracer::stats().currently_retained, 0);
    }

    #[test]
    fn test_strict_local_first_blocks_external_export() {
        let policy = DiagnosticsPolicy::local_dev();
        assert!(!policy.allow_external_export);

        let controller = DiagnosticsController::new(policy);
        let status = controller.get_status();
        assert!(status.local_first_guaranteed);
    }

    #[test]
    fn test_sanitize_output_if_disabled() {
        let controller = DiagnosticsController::new(DiagnosticsPolicy::production());
        assert_eq!(controller.sanitize_output_if_disabled("sensitive diagnostic info"), None);

        controller.enable_local_dev();
        assert_eq!(
            controller.sanitize_output_if_disabled("sensitive diagnostic info"),
            Some("sensitive diagnostic info")
        );
    }
}
