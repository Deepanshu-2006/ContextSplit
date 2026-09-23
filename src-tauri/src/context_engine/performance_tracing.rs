use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =========================================================================
// Operation Category
// =========================================================================

/// Explicit category of a major pipeline operation in ContextSplit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationCategory {
    /// Ingestion of user telemetry (window, app, url, file events).
    ActivityIngestion,
    /// Generation and ranking of workspace / context candidates.
    ContextCandidateGeneration,
    /// Privacy and quality gating (filtering noise, ephemeral hops, private apps).
    QualityGuard,
    /// Item-to-context membership assignment or inference.
    ContextAssignment,
    /// Processing user explicit feedback (confirmation, removal, rename, mute).
    FeedbackProcessing,
    /// Context lifecycle state detection and hysteresis evaluation.
    LifecycleDetection,
    /// Episodic memory creation, decay, and importance ranking.
    MemoryProcessing,
    /// Context snapshot capture, verification, and serialization.
    SnapshotProcessing,
    /// Synthesis and assembly of full contextual reconstruction.
    Reconstruction,
    /// Evaluating developer resume readiness and cognitive anchor status.
    ResumeReadiness,
}

impl OperationCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ActivityIngestion => "activity_ingestion",
            Self::ContextCandidateGeneration => "context_candidate_generation",
            Self::QualityGuard => "quality_guard",
            Self::ContextAssignment => "context_assignment",
            Self::FeedbackProcessing => "feedback_processing",
            Self::LifecycleDetection => "lifecycle_detection",
            Self::MemoryProcessing => "memory_processing",
            Self::SnapshotProcessing => "snapshot_processing",
            Self::Reconstruction => "reconstruction",
            Self::ResumeReadiness => "resume_readiness",
        }
    }

    /// Default baseline warning threshold for expensive execution in microseconds.
    pub fn default_expensive_threshold_us(&self) -> u64 {
        match self {
            Self::ActivityIngestion => 10_000,           // 10ms
            Self::ContextCandidateGeneration => 25_000,  // 25ms
            Self::QualityGuard => 5_000,                 // 5ms
            Self::ContextAssignment => 15_000,           // 15ms
            Self::FeedbackProcessing => 20_000,          // 20ms
            Self::LifecycleDetection => 15_000,          // 15ms
            Self::MemoryProcessing => 30_000,            // 30ms
            Self::SnapshotProcessing => 40_000,          // 40ms
            Self::Reconstruction => 50_000,              // 50ms
            Self::ResumeReadiness => 15_000,             // 15ms
        }
    }

    /// Baseline warning threshold for excessive database queries per single operation.
    pub fn default_excessive_query_threshold(&self) -> u32 {
        match self {
            Self::ActivityIngestion => 5,
            Self::ContextCandidateGeneration => 10,
            Self::QualityGuard => 3,
            Self::ContextAssignment => 8,
            Self::FeedbackProcessing => 12,
            Self::LifecycleDetection => 8,
            Self::MemoryProcessing => 15,
            Self::SnapshotProcessing => 20,
            Self::Reconstruction => 25,
            Self::ResumeReadiness => 10,
        }
    }
}

impl fmt::Display for OperationCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Performance Span
// =========================================================================

/// Record of a completed execution of an instrumented pipeline operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceSpan {
    /// Unique span identifier.
    pub id: String,
    /// High-level operation category.
    pub category: OperationCategory,
    /// Detailed operation name (e.g. "reconstruct_aurora_website", "score_candidate_matches").
    pub name: String,
    /// Target workspace / context identifier if scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    /// RFC3339 timestamp when operation started.
    pub started_at: String,
    /// Execution duration in microseconds.
    pub duration_us: u64,
    /// Number of database queries executed during this span.
    pub db_query_count: u32,
    /// Number of items/entities processed in this span.
    pub items_processed: usize,
    /// Whether the operation hit a cache instead of recomputing.
    pub cache_hit: bool,
    /// True if duration exceeded category threshold.
    pub is_expensive: bool,
    /// Sanitized structural fingerprint (e.g. hash of input IDs, no raw text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Structural diagnostics tags (e.g. "reasoning_model", "stage").
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

// =========================================================================
// Active Span (RAII Guard)
// =========================================================================

/// In-flight active span measurement.
pub struct ActiveSpan {
    pub id: String,
    pub category: OperationCategory,
    pub name: String,
    pub context_id: Option<String>,
    pub start_instant: Instant,
    pub started_at: String,
    pub db_query_count: u32,
    pub items_processed: usize,
    pub cache_hit: bool,
    pub custom_threshold_us: Option<u64>,
    pub fingerprint: Option<String>,
    pub tags: HashMap<String, String>,
    is_ended: bool,
}

impl ActiveSpan {
    pub fn new(category: OperationCategory, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            category,
            name: name.into(),
            context_id: None,
            start_instant: Instant::now(),
            started_at: Utc::now().to_rfc3339(),
            db_query_count: 0,
            items_processed: 0,
            cache_hit: false,
            custom_threshold_us: None,
            fingerprint: None,
            tags: HashMap::new(),
            is_ended: false,
        }
    }

    pub fn with_context(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }

    pub fn with_threshold_us(mut self, threshold_us: u64) -> Self {
        self.custom_threshold_us = Some(threshold_us);
        self
    }

    pub fn tag(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn add_query(&mut self) -> &mut Self {
        self.db_query_count += 1;
        self
    }

    pub fn increment_queries(&mut self, count: u32) -> &mut Self {
        self.db_query_count += count;
        self
    }

    pub fn set_items_processed(&mut self, items: usize) -> &mut Self {
        self.items_processed = items;
        self
    }

    pub fn set_cache_hit(&mut self, hit: bool) -> &mut Self {
        self.cache_hit = hit;
        self
    }

    pub fn set_fingerprint(&mut self, fingerprint: impl Into<String>) -> &mut Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }

    /// Explicitly completes the active span and records it in the global tracer.
    pub fn end(mut self) -> PerformanceSpan {
        let span = self.build_span();
        self.is_ended = true;
        get_performance_tracer().record(span.clone());
        span
    }

    fn build_span(&self) -> PerformanceSpan {
        let duration_us = self.start_instant.elapsed().as_micros() as u64;
        let threshold = self
            .custom_threshold_us
            .unwrap_or_else(|| self.category.default_expensive_threshold_us());

        PerformanceSpan {
            id: self.id.clone(),
            category: self.category,
            name: self.name.clone(),
            context_id: self.context_id.clone(),
            started_at: self.started_at.clone(),
            duration_us,
            db_query_count: self.db_query_count,
            items_processed: self.items_processed,
            cache_hit: self.cache_hit,
            is_expensive: duration_us >= threshold,
            fingerprint: self.fingerprint.clone(),
            tags: self.tags.clone(),
        }
    }
}

impl Drop for ActiveSpan {
    fn drop(&mut self) {
        if !self.is_ended {
            let span = self.build_span();
            self.is_ended = true;
            get_performance_tracer().record(span);
        }
    }
}

// =========================================================================
// Performance Tracer
// =========================================================================

/// Thread-safe in-memory performance tracing ring buffer.
pub struct PerformanceTracer {
    spans: RwLock<VecDeque<PerformanceSpan>>,
    capacity: usize,
}

impl PerformanceTracer {
    pub fn new(capacity: usize) -> Self {
        Self {
            spans: RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    /// Record a completed performance span into the ring buffer.
    pub fn record(&self, span: PerformanceSpan) {
        if let Ok(mut lock) = self.spans.write() {
            if lock.len() >= self.capacity {
                lock.pop_front();
            }
            lock.push_back(span);
        }
    }

    /// Returns a snapshot of the most recent recorded spans.
    pub fn get_recent_spans(&self, limit: usize) -> Vec<PerformanceSpan> {
        if let Ok(lock) = self.spans.read() {
            lock.iter().rev().take(limit).cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Returns all spans currently in the buffer.
    pub fn get_all_spans(&self) -> Vec<PerformanceSpan> {
        if let Ok(lock) = self.spans.read() {
            lock.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Returns all spans matching a specific context ID.
    pub fn get_spans_for_context(&self, context_id: &str) -> Vec<PerformanceSpan> {
        if let Ok(lock) = self.spans.read() {
            lock.iter()
                .filter(|s| s.context_id.as_deref() == Some(context_id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Returns all spans matching an operation category.
    pub fn get_spans_for_category(&self, category: OperationCategory) -> Vec<PerformanceSpan> {
        if let Ok(lock) = self.spans.read() {
            lock.iter()
                .filter(|s| s.category == category)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Clears the in-memory span buffer.
    pub fn clear(&self) {
        if let Ok(mut lock) = self.spans.write() {
            lock.clear();
        }
    }

    /// Number of spans currently stored.
    pub fn len(&self) -> usize {
        self.spans.read().map(|l| l.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Analyzes the recorded performance spans and produces an actionable diagnostics report.
    pub fn analyze(&self, window: Option<Duration>) -> PerformanceDiagnosticsReport {
        let spans = self.get_all_spans();
        let cutoff_str = window.map(|w| {
            let cut = Utc::now() - chrono::Duration::from_std(w).unwrap_or(chrono::Duration::days(1));
            cut.to_rfc3339()
        });

        let filtered_spans: Vec<PerformanceSpan> = spans
            .into_iter()
            .filter(|s| {
                if let Some(ref cut) = cutoff_str {
                    s.started_at >= *cut
                } else {
                    true
                }
            })
            .collect();

        PerformanceDiagnosticsReport::generate(&filtered_spans)
    }
}

// Global Performance Tracer Singleton
static GLOBAL_PERFORMANCE_TRACER: OnceLock<PerformanceTracer> = OnceLock::new();

pub fn get_performance_tracer() -> &'static PerformanceTracer {
    GLOBAL_PERFORMANCE_TRACER.get_or_init(|| PerformanceTracer::new(1000))
}

/// Helper function to start a measured active span.
pub fn start_span(category: OperationCategory, name: impl Into<String>) -> ActiveSpan {
    ActiveSpan::new(category, name)
}

// =========================================================================
// Performance Diagnostics & Anomalies
// =========================================================================

/// Summary of metrics for an individual operation category.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategoryPerformanceSummary {
    pub category: String,
    pub total_count: usize,
    pub total_duration_us: u64,
    pub avg_duration_us: u64,
    pub min_duration_us: u64,
    pub max_duration_us: u64,
    pub total_db_queries: u32,
    pub avg_db_queries: f32,
    pub total_items_processed: usize,
    pub cache_hit_count: usize,
    pub cache_hit_ratio: f32,
    pub expensive_count: usize,
}

/// Diagnostic warning for potential N+1 database querying pattern.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NPlusOneDiagnostic {
    pub category: String,
    pub operation_name: String,
    pub context_id: Option<String>,
    pub query_count: u32,
    pub items_processed: usize,
    pub queries_per_item: f32,
    pub description: String,
}

/// Diagnostic warning for unnecessary repeated recomputation without cache utilization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnnecessaryRecomputationDiagnostic {
    pub category: String,
    pub fingerprint: String,
    pub occurrence_count: usize,
    pub total_wasted_duration_us: u64,
    pub context_id: Option<String>,
    pub description: String,
}

/// Diagnostic warning for repeated semantic processing on identical inputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepeatedSemanticDiagnostic {
    pub operation_name: String,
    pub fingerprint: String,
    pub repeat_count: usize,
    pub context_id: Option<String>,
    pub description: String,
}

/// Diagnostic warning for single-operation excessive database query volume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExcessiveDbQueryDiagnostic {
    pub span_id: String,
    pub category: String,
    pub operation_name: String,
    pub query_count: u32,
    pub baseline_threshold: u32,
    pub description: String,
}

/// Comprehensive diagnostics report analyzing performance across the ContextSplit pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceDiagnosticsReport {
    pub generated_at: String,
    pub total_spans_analyzed: usize,
    pub category_summaries: HashMap<String, CategoryPerformanceSummary>,
    pub n_plus_one_warnings: Vec<NPlusOneDiagnostic>,
    pub unnecessary_recomputations: Vec<UnnecessaryRecomputationDiagnostic>,
    pub repeated_semantic_processing: Vec<RepeatedSemanticDiagnostic>,
    pub excessive_db_queries: Vec<ExcessiveDbQueryDiagnostic>,
    pub expensive_operations: Vec<PerformanceSpan>,
}

impl PerformanceDiagnosticsReport {
    pub fn generate(spans: &[PerformanceSpan]) -> Self {
        let mut summaries_map: HashMap<OperationCategory, Vec<&PerformanceSpan>> = HashMap::new();
        for span in spans {
            summaries_map.entry(span.category).or_default().push(span);
        }

        let mut category_summaries = HashMap::new();
        for (cat, cat_spans) in summaries_map {
            let total_count = cat_spans.len();
            if total_count == 0 {
                continue;
            }

            let mut total_duration = 0u64;
            let mut min_duration = u64::MAX;
            let mut max_duration = 0u64;
            let mut total_queries = 0u32;
            let mut total_items = 0usize;
            let mut cache_hits = 0usize;
            let mut expensive = 0usize;

            for s in &cat_spans {
                total_duration += s.duration_us;
                if s.duration_us < min_duration {
                    min_duration = s.duration_us;
                }
                if s.duration_us > max_duration {
                    max_duration = s.duration_us;
                }
                total_queries += s.db_query_count;
                total_items += s.items_processed;
                if s.cache_hit {
                    cache_hits += 1;
                }
                if s.is_expensive {
                    expensive += 1;
                }
            }

            let avg_duration = total_duration / (total_count as u64);
            let avg_queries = (total_queries as f32) / (total_count as f32);
            let cache_hit_ratio = (cache_hits as f32) / (total_count as f32);

            category_summaries.insert(
                cat.as_str().to_string(),
                CategoryPerformanceSummary {
                    category: cat.as_str().to_string(),
                    total_count,
                    total_duration_us: total_duration,
                    avg_duration_us: avg_duration,
                    min_duration_us: if min_duration == u64::MAX { 0 } else { min_duration },
                    max_duration_us: max_duration,
                    total_db_queries: total_queries,
                    avg_db_queries: avg_queries,
                    total_items_processed: total_items,
                    cache_hit_count: cache_hits,
                    cache_hit_ratio,
                    expensive_count: expensive,
                },
            );
        }

        // 1. Detect N+1 Operations: high query count relative to items processed (or items_processed >= 3 with queries >= items * 1.5)
        let mut n_plus_one_warnings = Vec::new();
        for s in spans {
            if s.items_processed >= 3 && s.db_query_count >= (s.items_processed as u32) {
                let ratio = (s.db_query_count as f32) / (s.items_processed as f32);
                if ratio >= 1.0 {
                    n_plus_one_warnings.push(NPlusOneDiagnostic {
                        category: s.category.as_str().to_string(),
                        operation_name: s.name.clone(),
                        context_id: s.context_id.clone(),
                        query_count: s.db_query_count,
                        items_processed: s.items_processed,
                        queries_per_item: ratio,
                        description: format!(
                            "Operation '{}' in category '{}' issued {} queries across {} items ({:.1} queries/item). Consider batching queries.",
                            s.name, s.category, s.db_query_count, s.items_processed, ratio
                        ),
                    });
                }
            }
        }

        // 2. Detect Unnecessary Recomputation: non-cached operations with identical fingerprint computed multiple times
        let mut fingerprint_groups: HashMap<String, Vec<&PerformanceSpan>> = HashMap::new();
        for s in spans {
            if let Some(ref fp) = s.fingerprint {
                if !s.cache_hit {
                    fingerprint_groups.entry(fp.clone()).or_default().push(s);
                }
            }
        }

        let mut unnecessary_recomputations = Vec::new();
        for (fp, group) in &fingerprint_groups {
            if group.len() >= 2 {
                let total_wasted_us: u64 = group.iter().skip(1).map(|s| s.duration_us).sum();
                let first = group[0];
                unnecessary_recomputations.push(UnnecessaryRecomputationDiagnostic {
                    category: first.category.as_str().to_string(),
                    fingerprint: fp.clone(),
                    occurrence_count: group.len(),
                    total_wasted_duration_us: total_wasted_us,
                    context_id: first.context_id.clone(),
                    description: format!(
                        "Fingerprint '{}' recomputed {} times in '{}' without cache hit, wasting {}µs.",
                        fp,
                        group.len(),
                        first.category,
                        total_wasted_us
                    ),
                });
            }
        }

        // 3. Detect Repeated Semantic Processing: multiple semantic/reasoning operations on same fingerprint or item
        let mut repeated_semantic_processing = Vec::new();
        for s in spans {
            if s.tags.contains_key("semantic_processing") || s.name.contains("semantic") || s.name.contains("reasoning") {
                if let Some(ref fp) = s.fingerprint {
                    if let Some(group) = fingerprint_groups.get(fp) {
                        if group.len() >= 2 && !repeated_semantic_processing.iter().any(|r: &RepeatedSemanticDiagnostic| r.fingerprint == *fp) {
                            repeated_semantic_processing.push(RepeatedSemanticDiagnostic {
                                operation_name: s.name.clone(),
                                fingerprint: fp.clone(),
                                repeat_count: group.len(),
                                context_id: s.context_id.clone(),
                                description: format!(
                                    "Semantic/reasoning operation '{}' executed {} times on identical fingerprint '{}'.",
                                    s.name,
                                    group.len(),
                                    fp
                                ),
                            });
                        }
                    }
                }
            }
        }

        // 4. Detect Excessive Database Queries
        let mut excessive_db_queries = Vec::new();
        for s in spans {
            let threshold = s.category.default_excessive_query_threshold();
            if s.db_query_count > threshold {
                excessive_db_queries.push(ExcessiveDbQueryDiagnostic {
                    span_id: s.id.clone(),
                    category: s.category.as_str().to_string(),
                    operation_name: s.name.clone(),
                    query_count: s.db_query_count,
                    baseline_threshold: threshold,
                    description: format!(
                        "Operation '{}' issued {} database queries, exceeding baseline threshold of {}.",
                        s.name, s.db_query_count, threshold
                    ),
                });
            }
        }

        // 5. Expensive Operations
        let expensive_operations: Vec<PerformanceSpan> = spans
            .iter()
            .filter(|s| s.is_expensive)
            .cloned()
            .collect();

        Self {
            generated_at: Utc::now().to_rfc3339(),
            total_spans_analyzed: spans.len(),
            category_summaries,
            n_plus_one_warnings,
            unnecessary_recomputations,
            repeated_semantic_processing,
            excessive_db_queries,
            expensive_operations,
        }
    }

    /// Formats the diagnostics report into a structured ASCII developer diagnostic string.
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        out.push_str("================================================================================\n");
        out.push_str("                  CONTEXTSPLIT PIPELINE PERFORMANCE REPORT                      \n");
        out.push_str("================================================================================\n");
        out.push_str(&format!("Generated At:          {}\n", self.generated_at));
        out.push_str(&format!("Total Spans Analyzed:  {}\n", self.total_spans_analyzed));
        out.push_str(&format!("Expensive Operations:  {}\n", self.expensive_operations.len()));
        out.push_str(&format!("N+1 Warnings:          {}\n", self.n_plus_one_warnings.len()));
        out.push_str(&format!("Unnecessary Recompute: {}\n", self.unnecessary_recomputations.len()));
        out.push_str(&format!("Excessive Queries:     {}\n\n", self.excessive_db_queries.len()));

        out.push_str("--------------------------------------------------------------------------------\n");
        out.push_str("CATEGORY BREAKDOWN\n");
        out.push_str("--------------------------------------------------------------------------------\n");
        out.push_str(&format!(
            "{:<28} | {:>6} | {:>9} | {:>9} | {:>8} | {:>8}\n",
            "Category", "Count", "Avg(µs)", "Max(µs)", "Queries", "CacheHit"
        ));
        out.push_str("-----------------------------+--------+-----------+-----------+----------+----------\n");

        for (_, summary) in &self.category_summaries {
            out.push_str(&format!(
                "{:<28} | {:>6} | {:>9} | {:>9} | {:>8} | {:>7.1}%\n",
                summary.category,
                summary.total_count,
                summary.avg_duration_us,
                summary.max_duration_us,
                summary.total_db_queries,
                summary.cache_hit_ratio * 100.0
            ));
        }

        if !self.n_plus_one_warnings.is_empty() {
            out.push_str("\n--------------------------------------------------------------------------------\n");
            out.push_str("N+1 QUERY PATTERNS DETECTED\n");
            out.push_str("--------------------------------------------------------------------------------\n");
            for (idx, w) in self.n_plus_one_warnings.iter().enumerate() {
                out.push_str(&format!(
                    "{}. [{}] {} -> {} items, {} queries ({:.1} q/item)\n",
                    idx + 1,
                    w.category,
                    w.operation_name,
                    w.items_processed,
                    w.query_count,
                    w.queries_per_item
                ));
            }
        }

        if !self.unnecessary_recomputations.is_empty() {
            out.push_str("\n--------------------------------------------------------------------------------\n");
            out.push_str("UNNECESSARY RECOMPUTATIONS DETECTED\n");
            out.push_str("--------------------------------------------------------------------------------\n");
            for (idx, u) in self.unnecessary_recomputations.iter().enumerate() {
                out.push_str(&format!(
                    "{}. [{}] Fingerprint: {} ({} runs, {}µs wasted)\n",
                    idx + 1,
                    u.category,
                    u.fingerprint,
                    u.occurrence_count,
                    u.total_wasted_duration_us
                ));
            }
        }

        if !self.repeated_semantic_processing.is_empty() {
            out.push_str("\n--------------------------------------------------------------------------------\n");
            out.push_str("REPEATED SEMANTIC PROCESSING DETECTED\n");
            out.push_str("--------------------------------------------------------------------------------\n");
            for (idx, r) in self.repeated_semantic_processing.iter().enumerate() {
                out.push_str(&format!(
                    "{}. {} -> {} times for fingerprint '{}'\n",
                    idx + 1,
                    r.operation_name,
                    r.repeat_count,
                    r.fingerprint
                ));
            }
        }

        if !self.excessive_db_queries.is_empty() {
            out.push_str("\n--------------------------------------------------------------------------------\n");
            out.push_str("EXCESSIVE DATABASE QUERIES DETECTED\n");
            out.push_str("--------------------------------------------------------------------------------\n");
            for (idx, e) in self.excessive_db_queries.iter().enumerate() {
                out.push_str(&format!(
                    "{}. [{}] {} -> {} queries (threshold: {})\n",
                    idx + 1,
                    e.category,
                    e.operation_name,
                    e.query_count,
                    e.baseline_threshold
                ));
            }
        }

        out.push_str("================================================================================\n");
        out
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_all_10_operation_categories_instrumented() {
        let tracer = PerformanceTracer::new(100);

        let categories = [
            OperationCategory::ActivityIngestion,
            OperationCategory::ContextCandidateGeneration,
            OperationCategory::QualityGuard,
            OperationCategory::ContextAssignment,
            OperationCategory::FeedbackProcessing,
            OperationCategory::LifecycleDetection,
            OperationCategory::MemoryProcessing,
            OperationCategory::SnapshotProcessing,
            OperationCategory::Reconstruction,
            OperationCategory::ResumeReadiness,
        ];

        for cat in categories {
            let mut span = ActiveSpan::new(cat, format!("test_{}", cat.as_str()));
            span.increment_queries(2);
            span.set_items_processed(5);
            let finished = span.end();
            tracer.record(finished);
        }

        let spans = tracer.get_all_spans();
        assert_eq!(spans.len(), 10);

        let report = tracer.analyze(None);
        assert_eq!(report.category_summaries.len(), 10);
        for cat in categories {
            assert!(report.category_summaries.contains_key(cat.as_str()));
        }
    }

    #[test]
    fn test_detect_n_plus_one_queries() {
        let tracer = PerformanceTracer::new(100);

        // Record an operation that processed 10 items and made 15 queries (1.5 queries/item)
        let mut span = ActiveSpan::new(OperationCategory::ContextAssignment, "assign_items_sequential");
        span.set_items_processed(10);
        span.increment_queries(15);
        tracer.record(span.end());

        let report = tracer.analyze(None);
        assert_eq!(report.n_plus_one_warnings.len(), 1);
        assert_eq!(report.n_plus_one_warnings[0].items_processed, 10);
        assert_eq!(report.n_plus_one_warnings[0].query_count, 15);
        assert!(report.n_plus_one_warnings[0].description.contains("batching"));
    }

    #[test]
    fn test_detect_unnecessary_recomputations() {
        let tracer = PerformanceTracer::new(100);

        // Record same operation fingerprint twice without cache hit
        let fp = "sha256:workspace_payx_items_v1";

        let mut span1 = ActiveSpan::new(OperationCategory::Reconstruction, "reconstruct_context");
        span1.set_fingerprint(fp);
        span1.set_cache_hit(false);
        tracer.record(span1.end());

        let mut span2 = ActiveSpan::new(OperationCategory::Reconstruction, "reconstruct_context");
        span2.set_fingerprint(fp);
        span2.set_cache_hit(false);
        tracer.record(span2.end());

        let report = tracer.analyze(None);
        assert_eq!(report.unnecessary_recomputations.len(), 1);
        assert_eq!(report.unnecessary_recomputations[0].fingerprint, fp);
        assert_eq!(report.unnecessary_recomputations[0].occurrence_count, 2);
    }

    #[test]
    fn test_detect_repeated_semantic_processing() {
        let tracer = PerformanceTracer::new(100);

        let fp = "sha256:semantic_intent_doc_v2";

        let mut span1 = ActiveSpan::new(OperationCategory::ContextCandidateGeneration, "extract_semantic_intent");
        span1.tag("semantic_processing", "true");
        span1.set_fingerprint(fp);
        span1.set_cache_hit(false);
        tracer.record(span1.end());

        let mut span2 = ActiveSpan::new(OperationCategory::ContextCandidateGeneration, "extract_semantic_intent");
        span2.tag("semantic_processing", "true");
        span2.set_fingerprint(fp);
        span2.set_cache_hit(false);
        tracer.record(span2.end());

        let report = tracer.analyze(None);
        assert_eq!(report.repeated_semantic_processing.len(), 1);
        assert_eq!(report.repeated_semantic_processing[0].repeat_count, 2);
    }

    #[test]
    fn test_detect_excessive_database_queries() {
        let tracer = PerformanceTracer::new(100);

        // QualityGuard threshold is 3 queries; issue 10 queries
        let mut span = ActiveSpan::new(OperationCategory::QualityGuard, "evaluate_quality_gate");
        span.increment_queries(10);
        tracer.record(span.end());

        let report = tracer.analyze(None);
        assert_eq!(report.excessive_db_queries.len(), 1);
        assert_eq!(report.excessive_db_queries[0].query_count, 10);
        assert_eq!(report.excessive_db_queries[0].baseline_threshold, 3);
    }

    #[test]
    fn test_bounded_ring_buffer_memory_safety() {
        let tracer = PerformanceTracer::new(5);

        for i in 0..10 {
            let span = ActiveSpan::new(OperationCategory::ActivityIngestion, format!("ingest_{}", i));
            tracer.record(span.end());
        }

        assert_eq!(tracer.len(), 5);
        let recent = tracer.get_recent_spans(10);
        assert_eq!(recent.len(), 5);
        assert_eq!(recent[0].name, "ingest_9");
    }

    #[test]
    fn test_raii_active_span_duration_recording() {
        let span = ActiveSpan::new(OperationCategory::ActivityIngestion, "quick_event");
        sleep(Duration::from_millis(5));
        let finished = span.end();

        assert!(finished.duration_us >= 4_000);
        assert_eq!(finished.name, "quick_event");
        assert_eq!(finished.category, OperationCategory::ActivityIngestion);
    }

    #[test]
    fn test_format_report_ascii_output() {
        let tracer = PerformanceTracer::new(100);

        let mut span1 = ActiveSpan::new(OperationCategory::Reconstruction, "reconstruct_context");
        span1.set_items_processed(12);
        span1.increment_queries(18);
        tracer.record(span1.end());

        let report = tracer.analyze(None);
        let formatted = report.format_report();

        assert!(formatted.contains("CONTEXTSPLIT PIPELINE PERFORMANCE REPORT"));
        assert!(formatted.contains("reconstruction"));
        assert!(formatted.contains("N+1 QUERY PATTERNS DETECTED"));
    }
}
