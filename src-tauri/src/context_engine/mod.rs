pub mod operations;
pub mod engine;
pub mod providers;
pub mod scoring;
pub mod relationships;
pub mod graph;
pub mod discovery;
pub mod timeline;
pub mod explanation;
pub mod temporal;
pub mod overview;
pub mod reasoning;
pub mod inbox_state;
pub mod attention;
pub mod evidence;
pub mod normalization;
pub mod diversity;
pub mod contradiction;
pub mod competition;
pub mod calibration;

pub use evidence::{
    ContextEvidence, ContextEvidenceCollector, ContextEvidenceSet, EntityReference,
    EvidenceCategory, EvidenceCollectorOptions, EvidenceExplanation, EvidenceProvenance,
    collect_item_evidence,
};
pub use normalization::{
    CategoryNormalizationConfig, EvidenceConflict, EvidenceNormalizer, NormalizedEvidence,
    NormalizedEvidenceSet, NormalizerConfig,
};
pub use diversity::{
    EvidenceDiversityCalculator, EvidenceDiversityResult, IndependentEvidenceGroup,
};
pub use contradiction::{
    ContradictionAnalyzer, ContradictionKind, ContextContradictionReport,
    MultiContextContradictionAnalysis,
};
pub use competition::{
    AmbiguityState, ContextCandidateSummary, ContextCompetitionAnalysis,
    ContextCompetitionAnalyzer,
};
pub use calibration::{
    CalibrationInput, CalibrationResult, ConfidenceBand, ConfidenceCalibrator,
};

#[cfg(test)]
pub mod workflow_tests;
#[cfg(test)]
pub mod evidence_pipeline_e2e_tests;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[derive(Debug, Clone)]
pub struct ActiveAppInfo {
    pub app_name: String,
    pub process_name: String,
    pub process_id: i32,
    pub window_title: Option<String>,
}

pub fn get_active_app() -> Option<ActiveAppInfo> {
    #[cfg(target_os = "macos")]
    {
        macos::active_app::get_active_app()
    }
    #[cfg(target_os = "windows")]
    {
        windows::active_app::get_active_app()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}
