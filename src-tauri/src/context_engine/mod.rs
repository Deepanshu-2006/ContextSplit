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
pub mod debugger;
pub mod transitions;
pub mod segments;
pub mod summary;
pub mod task_evidence;
pub mod task_extractor;
pub mod task_ranking;
pub mod task_transitions;
pub mod memory;
pub mod memory_query;
pub mod important_memory;
pub mod memory_importance;
pub mod memory_promotion;
pub mod memory_decay;
pub mod work_sequence;
pub mod workset;
pub mod context_state;
pub mod snapshot;
pub mod reconstruction;
pub mod quality_guard;
pub mod contamination;
pub mod ambiguity;
pub mod pending_activity;
pub mod correction;
pub mod lifecycle;
pub mod pause_detector;
pub mod dormancy;
pub mod resume_detector;
pub mod preservation;
pub mod session;
pub mod session_summary;
pub mod session_snapshot;
pub mod feedback;
pub mod decision_trace;
pub mod decision_explanation;
pub mod lifecycle_explanation;
pub mod memory_explanation;
pub mod feedback_explanation;
pub mod health_check;
pub mod cross_system_consistency;
pub mod performance_tracing;
pub mod diagnostics_guard;

pub use diagnostics_guard::{
    get_diagnostics_controller, DiagnosticsController, DiagnosticsMode, DiagnosticsPolicy,
    DiagnosticsSafetyStatus, CONTEXT_DIAGNOSTICS_SAFETY_DISCLAIMER,
};

pub use performance_tracing::{
    start_span, get_performance_tracer, ActiveSpan, CategoryPerformanceSummary,
    ExcessiveDbQueryDiagnostic, NPlusOneDiagnostic, OperationCategory,
    PerformanceDiagnosticsReport, PerformanceSpan, PerformanceTracer,
    RepeatedSemanticDiagnostic, UnnecessaryRecomputationDiagnostic,
};

pub use cross_system_consistency::{
    ContextIdentityMismatch, CrossSystemConsistencyChecker, CrossSystemConsistencyReport,
    SubsystemIdentityCheck, WorkspaceCrossSystemReport, CROSS_SYSTEM_CONSISTENCY_DISCLAIMER,
};

pub use health_check::{
    ContextHealthCheckService, ContextHealthIssue, ContextHealthReport, ContextHealthStatus,
    HealthIssueCategory, HealthIssueSeverity, CONTEXT_HEALTH_CHECK_DISCLAIMER,
};

pub use feedback_explanation::{
    ContextFeedbackEffectExplanation, ContextFeedbackExplanationService,
    CONTEXT_FEEDBACK_EFFECT_EXPLANATION_DISCLAIMER,
};

pub use memory_explanation::{
    ContextMemoryProvenanceExplanation, ContextMemoryExplanationService,
    CONTEXT_MEMORY_EXPLANATION_DISCLAIMER,
};

pub use lifecycle_explanation::{
    ContextLifecycleTransitionExplanation, ContextLifecycleExplanationService,
    LIFECYCLE_TRANSITION_EXPLANATION_DISCLAIMER,
};

pub use timeline::{
    format_timestamp_display, get_context_timeline, ContextTimelineCategory, ContextTimelineFilter,
    ContextTimelineInspector, ContextTimelineInspectorEvent, ContextTimelineInspectorReport,
    CONTEXT_TIMELINE_INSPECTOR_DISCLAIMER,
};

pub use decision_explanation::{
    ContextDecisionExplanation, ContextDecisionExplanationService,
    CONTEXT_DECISION_EXPLANATION_DISCLAIMER,
};

pub use decision_trace::{
    CandidateContextTrace, ContextDecisionTrace, ContextDecisionTraceBuilder, ContextDecisionTracer,
    DecisionTraceConfig, DecisionTraceFilter, DecisionTraceStats, DecisionType,
    EvidenceConsideredTrace, EvidenceStrengthRating, EvidenceStrengthSummary, FeedbackConsideredTrace,
    FinalDecisionOutcome, SourceEntityRef, CONTEXT_DECISION_TRACE_DISCLAIMER,
};

pub use feedback::{
    AdaptationVerdict, ContextDeterministicAdaptationEngine, ContextFeedbackEvent,
    ContextFeedbackInspector, ContextFeedbackRelevanceEngine, ContextFeedbackService,
    ContextNegativeFeedbackEvaluator, ContextPositiveFeedbackEngine,
    DeterministicAdaptationAssessment, EvidencePriorityTier, FeedbackAction,
    FeedbackApplicationResult, FeedbackInspectorContextReport, FeedbackInspectorEntry,
    FeedbackInspectorTargetReport, FeedbackMetadata, FeedbackPersistenceCategory,
    FeedbackPrecedenceEngine, FeedbackPrecedencePolicy, FeedbackRelevanceAssessment,
    FeedbackRelevanceConfig, FeedbackScope, FeedbackSource, FeedbackTargetRef,
    NegativeFeedbackAssessment, NegativeFeedbackVerdict, PositiveFeedbackAssessment,
    PrecedenceResolutionResult, PrioritizedEvidenceSignal, RelationshipLocalizationAssessment,
    CONTEXT_FEEDBACK_EVENT_DISCLAIMER,
};

pub use session_snapshot::{
    SessionSnapshotDecision, SessionSnapshotEvaluationInput, SessionSnapshotIntegrator,
    SessionSnapshotTriggerReason, SESSION_SNAPSHOT_INTEGRATION_DISCLAIMER,
};

pub use session_summary::{
    ContextSessionSummaryGenerator, LastMeaningfulActivitySummary, PreservedItemHint,
    SessionArtifactChange, SessionTaskSummary, StructuredSessionSummary,
    STRUCTURED_SESSION_SUMMARY_DISCLAIMER,
};

pub use session::{
    ContextSession, ContextSessionEvaluator, ContextSessionState, SessionEvaluationInput,
    SessionIntelligenceConfig, SessionSnapshotReference, SessionTaskReference,
    CONTEXT_SESSION_DISCLAIMER,
};

pub use preservation::{
    ContextPreservationService, PreservedContextHierarchy, PreservedSessionSegment,
    PreservedSnapshotRef, PreservedTaskState, CONTEXT_PRESERVATION_DISCLAIMER,
};

pub use resume_detector::{
    ContextResumeDetector, ResumeAssessment, ResumeDetectionConfig, ResumeEvidenceSignal,
    ResumeEvidenceType, ResumeEvaluationInput,
};

pub use dormancy::{
    DormancyDimensionScores, DormancyDistinctionAssessment, DormancyDistinctionEvaluator,
    DormancyEvaluationConfig, DormancyEvaluationInput, WorkCycleIndicators,
};

pub use pause_detector::{
    ContextPauseDetector, PauseAssessment, PauseCause, PauseDetectionConfig, PauseEvaluationInput,
};

pub use lifecycle::{
    ContextLifecycle, ContextLifecycleEvaluator, ContextLifecycleService, ContextLifecycleState,
    LifecycleEvaluationInput, LifecycleSourceEvidence,
};

pub use correction::{
    ContextCorrectionService, CorrectionDefenseVerdict, CorrectionEvidenceSignature,
    CorrectionFeedbackRecord,
};

pub use pending_activity::{
    ActivityEvidenceSignal, ContextualActivityStatus, DelayedResolutionConfig,
    DelayedResolutionResult, PendingActivityConfig, PendingActivityManager,
    PendingContextualActivity,
};

pub use ambiguity::{
    AmbiguityConfig, AmbiguityDecision, AmbiguityEvaluationInput, AmbiguityManager,
    AmbiguityResolutionResult, AmbiguityStatus, AmbiguousActivityRecord,
    CandidateContextAffinity, NewEvidenceInput,
};

pub use contamination::{
    extract_repo_slug, extract_search_query_terms, extract_topic_tokens, ContaminationCategory,
    ContaminationCheckInput, ContaminationDefenseResult, ContaminationGuard,
};

pub use quality_guard::{
    ContextQualityGuard, QualityGuardConfig, QualityGuardDecision, QualityGuardDimensionScores,
    QualityGuardInput, QualityGuardResult,
};

pub use reconstruction::{
    CachedReconstructionEntry, ContextReconstruction, ContextReconstructionBuilder,
    ContextReconstructionCache, ContextReconstructionQuality, ContextReconstructionService,
    ReconstructionActivityRef, ReconstructionConfidence, ReconstructionContextRef,
    ReconstructionContextSection, ReconstructionImportantMemoriesSection,
    ReconstructionImportantMemoryRef, ReconstructionMeaningfulActivitySection,
    ReconstructionQualityFactors, ReconstructionQualityRating, ReconstructionRecentActivitySection,
    ReconstructionResumeReadinessSection, ReconstructionSectionStatus, ReconstructionSnapshotRef,
    ReconstructionSnapshotSection, ReconstructionStateSection, ReconstructionTaskRef,
    ReconstructionTaskSection, ReconstructionWorksetItemRef, ReconstructionWorksetSection,
    get_global_reconstruction_cache, invalidate_reconstruction_cache,
    CONTEXT_RECONSTRUCTION_DISCLAIMER,
};

pub use memory_decay::{
    ContextMemoryDecayEvaluator, MemoryDecayInput, MemoryDecayPolicy, MemoryDecayResult,
};

pub use memory_promotion::{
    ContextMemoryPromotionEvaluator, MemoryPromotionAction, MemoryPromotionDecision,
    MemoryPromotionInput,
};

pub use memory_importance::{
    is_app_name_only, is_unrelated_popular_domain, ContextMemoryImportanceEvaluator,
    ContextMemoryQualityGuard, MemoryDimensionScores, MemoryImportanceEvaluation,
    MemoryImportanceInput, MemoryImportanceTier, MemoryQualityGuardResult,
};

pub use important_memory::{
    record_context_memory_control, ContextImportantMemory, ContextImportantMemoryBuilder,
    ContextImportantMemorySet, ImportantMemoryConfig, ImportantMemoryExplanation,
    ImportantMemoryMetadata, ImportantMemoryType, MemoryImportanceBand,
    UserMemoryControlAction,
};

pub use snapshot::{
    get_context_snapshot_history, get_global_snapshot_cache, get_global_snapshot_history,
    invalidate_snapshot_cache, record_snapshot_history, ContextSnapshot, ContextSnapshotBuilder,
    ContextSnapshotCache, ContextSnapshotComparator, ContextSnapshotDiff, ContextSnapshotHistory,
    ContextSnapshotHistoryRepository, ContextSnapshotQuality, ContextSnapshotQualityEvaluator,
    ContextSnapshotService, SnapshotActivityReference, SnapshotImportantMemoryReference,
    SnapshotQualityRating, SnapshotTaskReference, SnapshotTrigger, SnapshotTriggerClassifier,
    SnapshotTriggerCoalescer, SnapshotWorksetItemReference, CONTEXTUAL_SNAPSHOT_DISCLAIMER,
    DEFAULT_MAX_SNAPSHOT_RETENTION,
};

pub use context_state::{
    ContextResumeState, ContextState, ContextStateBuilder, ContextStateService, ContextStateValue,
    MeaningfulActivity, MeaningfulActivityResolver, MeaningfulActivityType,
    MeaningfulEntityReference, ResumeReadiness, ResumeReadinessAssessment,
    ResumeReadinessEvaluator,
};

pub use workset::{
    get_global_workset_cache, invalidate_workset_cache, ContextWorkset, ContextWorksetBuilder,
    ContextWorksetCache, ContextWorksetService, WorksetItemReference, WorksetItemRelevance,
    WorksetState,
};

pub use memory::{
    ContextMemory, ContextMemoryActivityState, ContextMemoryBuilder, ContextMemoryRepository,
    ContextMemoryService, InferredTaskState, InformationProvenance, MemoryActivitySequenceEntry,
    MemoryItemReference, MemoryTaskReference, MemoryTransitionReference, TaskConfidenceBand,
};
pub use memory_query::{
    ContextMemoryQueryService, ContextMemoryResponse, LastActiveTaskResponse,
};
pub use work_sequence::{
    format_activity_duration_estimate, format_date_label, format_time_window,
    RecentContextUsageRef, RecentTaskUsageRef, RecentWorkSequenceResponse,
    RecentWorkSequenceService, WorkSequenceDayGroup, WorkSequenceEntry,
};
pub use task_evidence::{
    clean_window_title, extract_salient_terms, TaskEvidence, TaskEvidenceCollector,
    TaskEvidenceCollectorOptions, TaskEvidenceSet, TaskEvidenceType,
};
pub use task_extractor::{
    humanize_identifier, is_generic_task_name, ExtractedTaskCandidate,
    TaskCandidateEvidenceRef, TaskCandidateExtractor, TaskExtractionOptions,
};
pub use task_ranking::{
    RankedTaskCandidate, TaskAmbiguityState, TaskCandidateRanker, TaskRankingConfig,
    TaskRankingDimensionScores, TaskRankingResult,
};
pub use task_transitions::{
    DetectedTaskTransition, PendingTaskTransition, TaskSnapshot, TaskTransitionConfig,
    TaskTransitionDetector, TaskTransitionEvaluationResult, TaskTransitionType,
};

pub use transitions::{
    format_duration_human, format_transition_summary, record_context_transition, ContextReference,
    ContextReturnDetails, ContextTransition, ContextTransitionBuilder,
    ContextTransitionDetector, ContextTransitionType, DetectedTransition, PendingTransition,
    TransitionEvaluationResult, TransitionStabilityConfig,
};
pub use segments::{
    ContextActivitySegment, ContextActivitySegmenter, SegmentItemReference, SegmentStreamEvent,
};
pub use summary::{
    format_duration_compact, ContextActivitySummary, ContextActivitySummaryService,
    ContextSequenceEntry, ContextUsageSummary, LongestSegmentSummary,
};

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
pub use debugger::{
    debug_item_context_confidence, CandidateConfidenceDebugItem, CompetitionDebugInfo,
    ContradictionDebugItem, ContextConfidenceDebugTrace, EvidenceDiversityDebugInfo,
    FinalResultDebugInfo, NormalizedEvidenceDebugItem, RawEvidenceDebugItem,
    ScoreContributionDebugItem,
};
pub use crate::db::models::{ContextInterpretationKind, NoContextReason};

#[cfg(test)]
pub mod workflow_tests;
#[cfg(test)]
pub mod evidence_pipeline_e2e_tests;
#[cfg(test)]
pub mod adversarial_tests;
#[cfg(test)]
pub mod task_adversarial_tests;
#[cfg(test)]
pub mod memory_e2e_tests;
#[cfg(test)]
pub mod workset_e2e_tests;
#[cfg(test)]
pub mod context_state_e2e_tests;
#[cfg(test)]
pub mod snapshot_e2e_tests;
#[cfg(test)]
pub mod memory_refinement_e2e_tests;
#[cfg(test)]
pub mod reconstruction_e2e_tests;
#[cfg(test)]
pub mod accuracy_dataset_tests;
pub mod evaluator;
#[cfg(test)]
pub mod workday_simulation_tests;
#[cfg(test)]
pub mod quality_guard_performance_tests;
#[cfg(test)]
pub mod phase1_e2e_validation_tests;
#[cfg(test)]
pub mod feedback_simulation_tests;
#[cfg(test)]
pub mod feedback_overlearning_tests;
#[cfg(test)]
pub mod decision_trace_scenario_tests;
#[cfg(test)]
pub mod phase1_observability_validation;
#[cfg(test)]
pub mod phase1_performance_tests;
#[cfg(test)]
pub mod phase1_user_journey_tests;

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
