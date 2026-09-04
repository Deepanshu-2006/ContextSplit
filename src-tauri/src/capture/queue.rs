use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::context_engine::engine::ContextEngine;
use crate::db::operations;
use crate::AppState;

/// The types of background work that can be scheduled for a captured item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BackgroundJobType {
    /// Generate embedding vector for the item
    EmbeddingGeneration,
    /// Generate deterministic and semantic relationships
    RelationshipCalculation,
    /// Run semantic analysis and context similarity scoring
    SemanticAnalysis,
    /// Run candidate clustering and discovery
    ContextDiscovery,
    /// Run AI reasoning on ambiguous / high-scoring context candidates
    AiReasoning,
    /// Complete suite of all background contextual tasks
    FullContextualPipeline,
}

impl std::fmt::Display for BackgroundJobType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmbeddingGeneration => write!(f, "embedding_generation"),
            Self::RelationshipCalculation => write!(f, "relationship_calculation"),
            Self::SemanticAnalysis => write!(f, "semantic_analysis"),
            Self::ContextDiscovery => write!(f, "context_discovery"),
            Self::AiReasoning => write!(f, "ai_reasoning"),
            Self::FullContextualPipeline => write!(f, "full_contextual_pipeline"),
        }
    }
}

/// A background job unit representing deferred processing for an item.
#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub id: String,
    pub item_id: String,
    pub job_type: BackgroundJobType,
    pub retry_count: usize,
    pub max_retries: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl BackgroundJob {
    pub fn new(item_id: impl Into<String>, job_type: BackgroundJobType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            item_id: item_id.into(),
            job_type,
            retry_count: 0,
            max_retries: 3,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }
}

/// Result of executing a background job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobResult {
    pub job_id: String,
    pub item_id: String,
    pub success: bool,
    pub retries: usize,
    pub error: Option<String>,
}

/// Thread-safe in-memory Background Processing Queue with deduplication and retry tracking.
#[derive(Debug, Clone, Default)]
pub struct BackgroundProcessingQueue {
    queue: Arc<Mutex<VecDeque<BackgroundJob>>>,
    in_flight: Arc<Mutex<HashSet<String>>>,
}

impl BackgroundProcessingQueue {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn job_key(item_id: &str, job_type: &BackgroundJobType) -> String {
        format!("{}:{}", item_id, job_type)
    }

    /// Enqueues a job for background processing.
    /// Returns `true` if enqueued, or `false` if an identical job is already in flight (avoiding duplicate processing).
    pub fn enqueue(&self, job: BackgroundJob) -> bool {
        let key = Self::job_key(&job.item_id, &job.job_type);
        let mut in_flight = match self.in_flight.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if in_flight.contains(&key) {
            // Duplicate processing avoided!
            return false;
        }

        in_flight.insert(key);

        let mut queue = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        queue.push_back(job);
        true
    }

    /// Pops the next pending job from the queue.
    pub fn dequeue(&self) -> Option<BackgroundJob> {
        let mut queue = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        queue.pop_front()
    }

    /// Checks if a job for the given item and type is currently in flight.
    pub fn is_in_flight(&self, item_id: &str, job_type: &BackgroundJobType) -> bool {
        let key = Self::job_key(item_id, job_type);
        let in_flight = match self.in_flight.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        in_flight.contains(&key)
    }

    /// Releases the in-flight tracking lock for a job.
    pub fn mark_completed(&self, item_id: &str, job_type: &BackgroundJobType) {
        let key = Self::job_key(item_id, job_type);
        let mut in_flight = match self.in_flight.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        in_flight.remove(&key);
    }

    /// Number of pending jobs.
    pub fn len(&self) -> usize {
        let queue = match self.queue.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        queue.len()
    }

    /// Checks if the queue has no pending jobs.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Executes a single background job in headless mode with error boundary and safe retries.
    pub fn process_job_headless(
        &self,
        conn: &Connection,
        context_engine: &ContextEngine,
        job: &BackgroundJob,
    ) -> BackgroundJobResult {
        let mut success = true;
        let mut error_msg: Option<String> = None;

        // Fetch the Item
        let item = match operations::get_item(conn, &job.item_id) {
            Ok(i) => i,
            Err(e) => {
                self.mark_completed(&job.item_id, &job.job_type);
                return BackgroundJobResult {
                    job_id: job.id.clone(),
                    item_id: job.item_id.clone(),
                    success: false,
                    retries: job.retry_count,
                    error: Some(format!("Item not found: {}", e)),
                };
            }
        };

        let settings = crate::db::settings::get_settings(conn).unwrap_or_default();

        // 1. Embedding Generation & Semantic Analysis
        if matches!(
            job.job_type,
            BackgroundJobType::EmbeddingGeneration
                | BackgroundJobType::SemanticAnalysis
                | BackgroundJobType::FullContextualPipeline
        ) {
            if settings.enable_semantic_processing {
                let provider_opt = context_engine.get_semantic_provider();
                if let Some(provider) = provider_opt {
                    let builder = crate::semantic::builder::SemanticContentBuilder::new();
                    let text_to_embed = builder.build(&item);

                    if !text_to_embed.trim().is_empty() {
                        let _ = operations::update_item_semantic_status(conn, &item.id, "processing", None);
                        match provider.generate_embedding(&text_to_embed) {
                            Ok(embedding) => {
                                let model_name = provider.status().model_name;
                                let _ = operations::save_item_embedding(
                                    conn, &item.id, &embedding, &model_name, &model_name,
                                );
                                let _ = operations::update_item_semantic_status(
                                    conn, &item.id, "ready", Some(&model_name),
                                );
                                let _ = crate::context_engine::relationships::generate_semantic_relationships(
                                    context_engine, conn, &item.id,
                                );
                            }
                            Err(e) => {
                                success = false;
                                error_msg = Some(format!("Embedding generation failed: {}", e));
                                let _ = operations::update_item_semantic_status(conn, &item.id, "failed", None);
                                let _ = operations::increment_semantic_retries(conn, &item.id);
                            }
                        }
                    } else {
                        let _ = operations::update_item_semantic_status(conn, &item.id, "ready", None);
                    }
                } else {
                    // No semantic provider available
                    let _ = operations::update_item_semantic_status(conn, &item.id, "failed", None);
                    let _ = operations::increment_semantic_retries(conn, &item.id);
                }
            }
        }

        // 2. Deterministic Relationship Calculation
        if matches!(
            job.job_type,
            BackgroundJobType::RelationshipCalculation | BackgroundJobType::FullContextualPipeline
        ) {
            let _ = crate::context_engine::relationships::generate_relationships(conn, &item.id);
        }

        // 3. Context Discovery & Scoring Recalculation
        if matches!(
            job.job_type,
            BackgroundJobType::ContextDiscovery | BackgroundJobType::FullContextualPipeline
        ) {
            if let Some(ws_id) = &item.workspace_id {
                let _ = crate::context_engine::scoring::recalculate_workspace_suggested_memberships(
                    conn, ws_id, Some(&item.id),
                );
            }
            let _ = crate::context_engine::discovery::run_discovery(conn);
        }

        // 4. AI Reasoning (Gracefully safe: failure never breaks capture)
        if matches!(
            job.job_type,
            BackgroundJobType::AiReasoning | BackgroundJobType::FullContextualPipeline
        ) {
            if settings.enable_ai_reasoning {
                let _ = context_engine.analyze_item_context_ai(conn, &item.id);
            }
        }

        self.mark_completed(&job.item_id, &job.job_type);

        BackgroundJobResult {
            job_id: job.id.clone(),
            item_id: job.item_id.clone(),
            success,
            retries: job.retry_count,
            error: error_msg,
        }
    }

    /// Drains and processes all pending background jobs in the queue synchronously.
    pub fn process_all_pending_headless(
        &self,
        conn: &Connection,
        context_engine: &ContextEngine,
    ) -> Vec<BackgroundJobResult> {
        let mut results = Vec::new();
        while let Some(job) = self.dequeue() {
            let res = self.process_job_headless(conn, context_engine, &job);
            results.push(res);
        }
        results
    }

    /// Retries failed items recorded in the database if retries < max_retries.
    pub fn retry_failed_items_headless(
        &self,
        conn: &Connection,
        context_engine: &ContextEngine,
        max_retries: i32,
    ) -> usize {
        let failed_items = operations::get_pending_or_failed_items(conn, max_retries).unwrap_or_default();
        let count = failed_items.len();
        for item_id in failed_items {
            let job = BackgroundJob::new(item_id, BackgroundJobType::FullContextualPipeline);
            let _ = self.process_job_headless(conn, context_engine, &job);
        }
        count
    }

    /// Spawns background worker in live Tauri application mode.
    pub fn spawn_app_job(
        &self,
        app_handle: AppHandle,
        job: BackgroundJob,
    ) {
        let queue_clone = self.clone();
        tauri::async_runtime::spawn(async move {
            let job_clone = job.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    if let (Ok(conn), Ok(ce)) = (state.db_conn.lock(), state.context_engine.lock()) {
                        let _res = queue_clone.process_job_headless(&conn, &ce, &job_clone);
                    } else {
                        queue_clone.mark_completed(&job_clone.item_id, &job_clone.job_type);
                    }
                } else {
                    queue_clone.mark_completed(&job_clone.item_id, &job_clone.job_type);
                }
            })
            .await;
        });
    }
}
