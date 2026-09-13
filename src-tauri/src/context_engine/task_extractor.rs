use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

use crate::context_engine::evidence::{EntityType, EvidenceProvenance};
use crate::context_engine::segments::{ContextActivitySegment, SegmentItemReference};
use crate::context_engine::task_evidence::{
    clean_window_title, extract_file_extension, extract_salient_terms, TaskEvidence,
    TaskEvidenceCollector, TaskEvidenceCollectorOptions, TaskEvidenceSet, TaskEvidenceType,
};
use crate::context_engine::memory::ContextMemory;
use crate::context_engine::transitions::ContextSnapshot;

// =========================================================================
// Data Models & Configuration
// =========================================================================

/// Reference to a piece of contextual evidence backing a task candidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCandidateEvidenceRef {
    pub evidence_type: TaskEvidenceType,
    pub provenance: EvidenceProvenance,
    pub summary: String,
    pub strength: f64,
    pub source_entity: Option<String>,
}

impl TaskCandidateEvidenceRef {
    pub fn from_task_evidence(ev: &TaskEvidence) -> Self {
        Self {
            evidence_type: ev.evidence_type,
            provenance: ev.provenance,
            summary: ev.summary.clone(),
            strength: ev.strength,
            source_entity: ev.source_entity.as_ref().map(|e| e.entity_id.clone()),
        }
    }
}

/// An inferred, evidence-backed task candidate extracted from contextual activity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedTaskCandidate {
    /// Actionable, specific task title (e.g., "Implementing JWT Auth Service", "CS106B Recursion Assignment")
    pub title: String,
    /// Extracted or inferred action verb (e.g., "Implementing", "Reviewing", "Debugging", "Researching", "Booking")
    pub action_verb: Option<String>,
    /// Target subject, deliverable, or entity (e.g., "JWT Auth Service", "CS106B Recursion Assignment", "PR #42")
    pub target_subject: Option<String>,
    /// Initial relevance / confidence score (bounded [0.0, 1.0])
    pub relevance_score: f64,
    /// Evidence references backing this candidate
    pub evidence_references: Vec<TaskCandidateEvidenceRef>,
    /// Associated item IDs in the contextual cluster
    pub item_ids: Vec<String>,
    /// Primary evidence types that contributed to this extraction
    pub primary_evidence_types: Vec<TaskEvidenceType>,
    /// Salient terms supporting this candidate
    pub salient_terms: Vec<String>,
    /// Parent context/workspace ID if available
    pub context_id: Option<String>,
    /// Diagnostic and attribution metadata
    pub metadata: Value,
}

/// Configuration options for the Task Candidate Extractor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExtractionOptions {
    /// Minimum relevance score required to return a candidate (default: 0.35)
    pub min_relevance_score: f64,
    /// Maximum number of distinct task candidates to return (default: 5)
    pub max_candidates: usize,
    /// Whether to automatically specialize generic titles (default: true)
    pub enable_specialization: bool,
    /// Minimum relationship edge strength to consider items related (default: 0.50)
    pub min_relationship_strength: f64,
}

impl Default for TaskExtractionOptions {
    fn default() -> Self {
        Self {
            min_relevance_score: 0.35,
            max_candidates: 5,
            enable_specialization: true,
            min_relationship_strength: 0.50,
        }
    }
}

// =========================================================================
// Anti-Generic Term Filter
// =========================================================================

/// Blacklisted generic terms that cannot standalone as task names without specialization.
static GENERIC_TASK_TERMS: &[&str] = &[
    "work",
    "project",
    "coding",
    "research",
    "design",
    "website",
    "general",
    "task",
    "misc",
    "miscellaneous",
    "development",
    "study",
    "notes",
    "new tab",
    "untitled",
    "untitled document",
    "document",
    "google search",
    "home",
    "dashboard",
    "app",
    "application",
    "file",
    "tab",
    "browsing",
    "web browsing",
    "code",
    "programming",
    "computer",
    "activity",
    "unknown",
    "react",
    "vue",
    "angular",
    "rust",
    "python",
    "javascript",
    "typescript",
    "frontend",
    "backend",
    "https",
    "http",
    "www",
    "com",
    "org",
    "net",
    "io",
    "html",
    "htm",
    "page",
    "search",
    "github",
    "repo",
    "repository",
    "git",
    "chrome",
    "google chrome",
    "safari",
    "firefox",
    "figma",
    "slack",
    "notion",
    "terminal",
    "vscode",
    "visual studio code",
    "browser",
];

/// Checks whether a phrase or title is purely generic.
pub fn is_generic_task_name(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return true;
    }

    // Direct match against generic list
    for generic in GENERIC_TASK_TERMS {
        if &lower == generic {
            return true;
        }
    }

    // Single-word check or combinations of generic terms (e.g. "Work Project", "Coding Website")
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return true;
    }

    // If ALL words in the title are generic stop words or in the generic dictionary
    let all_generic = words.iter().all(|w| {
        GENERIC_TASK_TERMS.contains(w)
            || ["a", "an", "the", "in", "on", "at", "for", "to", "and", "of", "with"].contains(w)
    });

    all_generic
}

// =========================================================================
// Task Candidate Extractor
// =========================================================================

/// Deterministic, evidence-backed extractor that infers task candidates from
/// activity segments, context memory, and contextual evidence sets.
pub struct TaskCandidateExtractor;

impl TaskCandidateExtractor {
    /// Extracts task candidates from a continuous `ContextActivitySegment`.
    pub fn extract_from_segment(
        conn: &Connection,
        segment: &ContextActivitySegment,
        options: Option<&TaskExtractionOptions>,
    ) -> Result<Vec<ExtractedTaskCandidate>, String> {
        let opts = options.cloned().unwrap_or_default();
        let context_id = segment.context.as_ref().map(|c| c.id.clone());

        // Collect task evidence for this segment's context and items
        let collector_opts = TaskEvidenceCollectorOptions {
            max_recent_items: (segment.item_references.len() + 10).max(25),
            min_relationship_strength: opts.min_relationship_strength,
            ..Default::default()
        };

        let evidence_set = if let Some(ref cid) = context_id {
            TaskEvidenceCollector::with_options(collector_opts)
                .collect_for_context(conn, cid)
                .map_err(|e| format!("Failed to collect evidence for segment context: {}", e))?
        } else {
            // Unassigned segment: collect evidence from individual item references
            let item_ids: Vec<String> = segment
                .item_references
                .iter()
                .map(|r| r.item_id.clone())
                .collect();
            Self::collect_evidence_for_item_ids(conn, &item_ids, opts.min_relationship_strength)?
        };

        // If segment has specific item references, restrict extraction to those items
        let segment_item_ids: HashSet<String> = segment
            .item_references
            .iter()
            .map(|r| r.item_id.clone())
            .collect();

        let mut filtered_evidence = TaskEvidenceSet::new(context_id.clone(), None);
        for ev in &evidence_set.evidence {
            let matches_item = ev
                .source_entity
                .as_ref()
                .map(|e| segment_item_ids.contains(&e.entity_id))
                .unwrap_or(true);
            if matches_item || segment_item_ids.is_empty() {
                filtered_evidence.add_evidence(ev.clone());
            }
        }

        let candidates = Self::extract_from_evidence(&filtered_evidence, Some(&opts));
        Ok(candidates)
    }

    /// Extracts task candidates from an in-memory `TaskEvidenceSet`.
    pub fn extract_from_evidence(
        evidence_set: &TaskEvidenceSet,
        options: Option<&TaskExtractionOptions>,
    ) -> Vec<ExtractedTaskCandidate> {
        let opts = options.cloned().unwrap_or_default();
        let context_id = evidence_set.context_id.clone();

        if evidence_set.evidence.is_empty() {
            return Vec::new();
        }

        // 1. Group evidence into coherent clusters (by domain/issue, file stem, or salient term)
        let clusters = Self::partition_evidence_into_clusters(evidence_set);

        let mut candidates = Vec::new();

        for cluster in clusters {
            if let Some(candidate) =
                Self::extract_candidate_from_cluster(&cluster, context_id.as_deref(), &opts)
            {
                if candidate.relevance_score >= opts.min_relevance_score {
                    candidates.push(candidate);
                }
            }
        }

        // If no cluster candidate met threshold but evidence exists, create a global synthesized candidate
        if candidates.is_empty() {
            if let Some(fallback) =
                Self::extract_candidate_from_cluster(&evidence_set.evidence, context_id.as_deref(), &opts)
            {
                if fallback.relevance_score >= opts.min_relevance_score {
                    candidates.push(fallback);
                }
            }
        }

        // Merge candidates that refer to the same subject or have high term overlap
        let mut merged_candidates = Self::merge_overlapping_candidates(candidates);

        // Sort descending by relevance score
        merged_candidates.sort_by(|a, b| {
            b.relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        merged_candidates.truncate(opts.max_candidates);
        merged_candidates
    }

    /// Merges co-referent task candidates that target the same deliverable or share salient terms.
    fn merge_overlapping_candidates(
        candidates: Vec<ExtractedTaskCandidate>,
    ) -> Vec<ExtractedTaskCandidate> {
        let mut merged: Vec<ExtractedTaskCandidate> = Vec::new();

        for cand in candidates {
            let mut matched_idx = None;
            for (idx, existing) in merged.iter().enumerate() {
                let same_subject = match (&existing.target_subject, &cand.target_subject) {
                    (Some(s1), Some(s2)) => {
                        !s1.trim().is_empty()
                            && !is_generic_task_name(s1)
                            && s1.eq_ignore_ascii_case(s2)
                    }
                    _ => false,
                };
                let same_title = !existing.title.trim().is_empty()
                    && !is_generic_task_name(&existing.title)
                    && existing.title.eq_ignore_ascii_case(&cand.title);

                let item_overlap = {
                    let set1: HashSet<_> = existing.item_ids.iter().collect();
                    let set2: HashSet<_> = cand.item_ids.iter().collect();
                    let common = set1.intersection(&set2).count();
                    common > 0 && (common * 2 >= set1.len() || common * 2 >= set2.len())
                };

                let title_word_overlap = {
                    let words1: HashSet<_> = existing
                        .title
                        .split(|c: char| !c.is_alphanumeric())
                        .map(|s| s.to_lowercase())
                        .filter(|w| w.len() >= 3 && !is_generic_task_name(w))
                        .collect();
                    let words2: HashSet<_> = cand
                        .title
                        .split(|c: char| !c.is_alphanumeric())
                        .map(|s| s.to_lowercase())
                        .filter(|w| w.len() >= 3 && !is_generic_task_name(w))
                        .collect();
                    let count = words1.intersection(&words2).count();
                    count >= 2 || (!words1.is_empty() && count == words1.len() && count == words2.len())
                };

                if same_subject || same_title || item_overlap || title_word_overlap {
                    matched_idx = Some(idx);
                    break;
                }
            }

            if let Some(idx) = matched_idx {
                let existing = &mut merged[idx];
                // Union item IDs
                for id in cand.item_ids {
                    if !existing.item_ids.contains(&id) {
                        existing.item_ids.push(id);
                    }
                }
                // Union evidence references
                for ev in cand.evidence_references {
                    if !existing
                        .evidence_references
                        .iter()
                        .any(|e| e.summary == ev.summary)
                    {
                        existing.evidence_references.push(ev);
                    }
                }
                // Union primary evidence types
                for pt in cand.primary_evidence_types {
                    if !existing.primary_evidence_types.contains(&pt) {
                        existing.primary_evidence_types.push(pt);
                    }
                }
                // Union salient terms
                for st in cand.salient_terms {
                    if !existing
                        .salient_terms
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case(&st))
                    {
                        existing.salient_terms.push(st);
                    }
                }
                // Combined relevance score (multi-source corroboration boost)
                existing.relevance_score =
                    (existing.relevance_score.max(cand.relevance_score) + 0.08).min(0.98);
                // Prefer more descriptive or specific title
                if cand.title.len() > existing.title.len() && !is_generic_task_name(&cand.title) {
                    existing.title = cand.title;
                    if cand.action_verb.is_some() {
                        existing.action_verb = cand.action_verb;
                    }
                    if cand.target_subject.is_some() {
                        existing.target_subject = cand.target_subject;
                    }
                }
            } else {
                merged.push(cand);
            }
        }

        merged
    }

    /// Extracts task candidates from an explicit list of item IDs.
    pub fn extract_from_items(
        conn: &Connection,
        item_ids: &[String],
        context_id: Option<&str>,
        options: Option<&TaskExtractionOptions>,
    ) -> Result<Vec<ExtractedTaskCandidate>, String> {
        let opts = options.cloned().unwrap_or_default();
        let evidence_set =
            Self::collect_evidence_for_item_ids(conn, item_ids, opts.min_relationship_strength)?;

        let mut set_with_ctx = evidence_set;
        if set_with_ctx.context_id.is_none() && context_id.is_some() {
            set_with_ctx.context_id = context_id.map(|s| s.to_string());
        }

        Ok(Self::extract_from_evidence(&set_with_ctx, Some(&opts)))
    }

    /// Extracts task candidates from a `ContextSnapshot` working memory.
    pub fn extract_from_memory(
        conn: &Connection,
        snapshot: &ContextSnapshot,
        item_refs: Option<&[SegmentItemReference]>,
        options: Option<&TaskExtractionOptions>,
    ) -> Result<Vec<ExtractedTaskCandidate>, String> {
        let opts = options.cloned().unwrap_or_default();
        let context_id = Some(snapshot.reference.id.clone());

        let evidence_set = if let Some(refs) = item_refs {
            let item_ids: Vec<String> = refs.iter().map(|r| r.item_id.clone()).collect();
            let mut ev =
                Self::collect_evidence_for_item_ids(conn, &item_ids, opts.min_relationship_strength)?;
            ev.context_id = context_id.clone();
            ev
        } else {
            TaskEvidenceCollector::with_options(TaskEvidenceCollectorOptions {
                min_relationship_strength: opts.min_relationship_strength,
                ..Default::default()
            })
            .collect_for_context(conn, &snapshot.reference.id)
            .map_err(|e| format!("Failed to collect memory evidence: {}", e))?
        };

        Ok(Self::extract_from_evidence(&evidence_set, Some(&opts)))
    }

    /// Extracts task candidates from a `ContextMemory` working memory.
    pub fn extract_from_context_memory(
        conn: &Connection,
        memory: &ContextMemory,
        item_refs: Option<&[SegmentItemReference]>,
        options: Option<&TaskExtractionOptions>,
    ) -> Result<Vec<ExtractedTaskCandidate>, String> {
        let opts = options.cloned().unwrap_or_default();
        let context_id = memory.context_id_str().map(|s| s.to_string());

        let evidence_set = if let Some(refs) = item_refs {
            let item_ids: Vec<String> = refs.iter().map(|r| r.item_id.clone()).collect();
            let mut ev =
                Self::collect_evidence_for_item_ids(conn, &item_ids, opts.min_relationship_strength)?;
            ev.context_id = context_id;
            ev
        } else if let Some(cid) = memory.context_id_str() {
            TaskEvidenceCollector::with_options(TaskEvidenceCollectorOptions {
                min_relationship_strength: opts.min_relationship_strength,
                ..Default::default()
            })
            .collect_for_context(conn, cid)
            .map_err(|e| format!("Failed to collect context memory evidence: {}", e))?
        } else {
            TaskEvidenceSet::new(None, None)
        };

        Ok(Self::extract_from_evidence(&evidence_set, Some(&opts)))
    }

    // =========================================================================
    // Internal Clustering & Inference
    // =========================================================================

    /// Helper to collect task evidence from a list of item IDs.
    fn collect_evidence_for_item_ids(
        conn: &Connection,
        item_ids: &[String],
        min_relationship_strength: f64,
    ) -> Result<TaskEvidenceSet, String> {
        let mut evidence_set = TaskEvidenceSet::new(None, None);
        let opts = TaskEvidenceCollectorOptions {
            min_relationship_strength,
            ..Default::default()
        };
        let collector = TaskEvidenceCollector::with_options(opts);

        for id in item_ids {
            match collector.collect_for_item(conn, id) {
                Ok(ev) => {
                    for item in ev.evidence {
                        evidence_set.add_evidence(item);
                    }
                }
                Err(e) => return Err(format!("Error collecting item evidence {}: {}", id, e)),
            }
        }

        Ok(evidence_set)
    }

    /// Partitions an evidence set into clusters representing distinct possible tasks.
    fn partition_evidence_into_clusters(evidence_set: &TaskEvidenceSet) -> Vec<Vec<TaskEvidence>> {
        if evidence_set.evidence.is_empty() {
            return Vec::new();
        }

        let mut item_groups: HashMap<String, Vec<TaskEvidence>> = HashMap::new();
        let mut non_item_evidence: Vec<TaskEvidence> = Vec::new();

        for ev in &evidence_set.evidence {
            if let Some(ref entity) = ev.source_entity {
                item_groups
                    .entry(entity.entity_id.clone())
                    .or_default()
                    .push(ev.clone());
            } else if let Some(title) = ev.attributes.get("title").and_then(|v| v.as_str()) {
                item_groups
                    .entry(format!("title:{}", title))
                    .or_default()
                    .push(ev.clone());
            } else if let Some(fn_str) = ev.attributes.get("filename").and_then(|v| v.as_str()) {
                item_groups
                    .entry(format!("file:{}", fn_str))
                    .or_default()
                    .push(ev.clone());
            } else if let Some(url_str) = ev.attributes.get("url").and_then(|v| v.as_str()) {
                item_groups
                    .entry(format!("url:{}", url_str))
                    .or_default()
                    .push(ev.clone());
            } else {
                non_item_evidence.push(ev.clone());
            }
        }

        // Build adjacency graph for connected items using ItemRelationships and SemanticSimilarity
        let item_keys: Vec<String> = item_groups.keys().cloned().collect();
        let mut parent_map: HashMap<String, String> = HashMap::new();
        for k in &item_keys {
            parent_map.insert(k.clone(), k.clone());
        }

        fn find_root(parent: &mut HashMap<String, String>, k: &str) -> String {
            let p = parent.get(k).cloned().unwrap_or_else(|| k.to_string());
            if p == k {
                k.to_string()
            } else {
                let root = find_root(parent, &p);
                parent.insert(k.to_string(), root.clone());
                root
            }
        }

        fn union_roots(parent: &mut HashMap<String, String>, a: &str, b: &str) {
            let root_a = find_root(parent, a);
            let root_b = find_root(parent, b);
            if root_a != root_b {
                parent.insert(root_a, root_b);
            }
        }

        // Connect items that share explicit relationships in evidence
        for ev in &evidence_set.evidence {
            if ev.evidence_type == TaskEvidenceType::ItemRelationships
                || ev.evidence_type == TaskEvidenceType::SemanticSimilarity
            {
                if let (Some(s1), Some(s2)) = (
                    ev.attributes.get("source_item_id").and_then(|v| v.as_str()),
                    ev.attributes.get("target_item_id").and_then(|v| v.as_str()),
                ) {
                    if item_groups.contains_key(s1) && item_groups.contains_key(s2) {
                        union_roots(&mut parent_map, s1, s2);
                    }
                }
            }
        }

        // Connect items that share significant salient words in their titles or content
        for i in 0..item_keys.len() {
            for j in (i + 1)..item_keys.len() {
                let k1 = &item_keys[i];
                let k2 = &item_keys[j];
                let evs1 = &item_groups[k1];
                let evs2 = &item_groups[k2];

                let mut words1: HashSet<String> = HashSet::new();
                for e in evs1 {
                    for attr_key in &["title", "filename", "snippet", "url", "domain"] {
                        if let Some(val) = e.attributes.get(*attr_key).and_then(|v| v.as_str()) {
                            for w in val.split(|c: char| !c.is_alphanumeric()) {
                                let lower = w.to_lowercase();
                                if lower.len() >= 3 && !is_generic_task_name(&lower) {
                                    words1.insert(lower);
                                }
                            }
                        }
                    }
                }

                let mut words2: HashSet<String> = HashSet::new();
                for e in evs2 {
                    for attr_key in &["title", "filename", "snippet", "url", "domain"] {
                        if let Some(val) = e.attributes.get(*attr_key).and_then(|v| v.as_str()) {
                            for w in val.split(|c: char| !c.is_alphanumeric()) {
                                let lower = w.to_lowercase();
                                if lower.len() >= 3 && !is_generic_task_name(&lower) {
                                    words2.insert(lower);
                                }
                            }
                        }
                    }
                }

                let common = words1.intersection(&words2).count();
                if common >= 2
                    || (!words1.is_empty() && common == words1.len() && common == words2.len())
                {
                    union_roots(&mut parent_map, k1, k2);
                }
            }
        }

        // Group item evidence by connected component root
        let mut cluster_map: HashMap<String, Vec<TaskEvidence>> = HashMap::new();
        for (item_id, evs) in item_groups {
            let root = find_root(&mut parent_map, &item_id);
            cluster_map.entry(root).or_default().extend(evs);
        }

        // Attach global non-item evidence to each cluster
        let mut clusters: Vec<Vec<TaskEvidence>> = Vec::new();
        for (_root, mut cluster_items) in cluster_map {
            for non_item in &non_item_evidence {
                cluster_items.push(non_item.clone());
            }
            clusters.push(cluster_items);
        }

        if clusters.is_empty() {
            clusters.push(evidence_set.evidence.clone());
        }

        clusters
    }

    /// Extracts a single cohesive `ExtractedTaskCandidate` from a cluster of evidence.
    fn extract_candidate_from_cluster(
        cluster: &[TaskEvidence],
        context_id: Option<&str>,
        options: &TaskExtractionOptions,
    ) -> Option<ExtractedTaskCandidate> {
        if cluster.is_empty() {
            return None;
        }

        let pr_issue_intent = Self::extract_pr_or_issue_intent(cluster);
        let filename_intent = Self::extract_filename_intent(cluster);
        let window_intent = Self::extract_window_title_intent(cluster);
        let content_intent = Self::extract_content_collocation_intent(cluster);

        let (raw_title, verb, subject, source_type) = if let Some(pi) = pr_issue_intent {
            (pi.0, pi.1, pi.2, TaskEvidenceType::UrlsDomains)
        } else if let Some(fi) = filename_intent {
            (fi.0, fi.1, fi.2, TaskEvidenceType::Filenames)
        } else if let Some(wi) = window_intent {
            (wi.0, wi.1, wi.2, TaskEvidenceType::ApplicationContext)
        } else if let Some(ci) = content_intent {
            (ci.0, ci.1, ci.2, TaskEvidenceType::RecentItemContent)
        } else {
            let generic_title = "Contextual Activity".to_string();
            (generic_title, None, None, TaskEvidenceType::RecentItemContent)
        };

        // Extract repeated salient terms from the cluster
        let text_corpus: Vec<String> = cluster
            .iter()
            .filter_map(|e| {
                if let Some(snip) = e.attributes.get("snippet").and_then(|v| v.as_str()) {
                    Some(snip.to_string())
                } else if let Some(t) = e.attributes.get("title").and_then(|v| v.as_str()) {
                    Some(t.to_string())
                } else if let Some(wt) = e.attributes.get("window_title").and_then(|v| v.as_str()) {
                    Some(wt.to_string())
                } else {
                    None
                }
            })
            .collect();
        let salient_terms = extract_salient_terms(&text_corpus, 5);

        // Apply Anti-Generic Filtering & Specialization
        let (final_title, final_verb, final_subject, is_specialized) = if is_generic_task_name(&raw_title)
        {
            if options.enable_specialization {
                Self::specialize_generic_title(&raw_title, cluster, &salient_terms)
            } else {
                (raw_title.clone(), verb.clone(), subject.clone(), false)
            }
        } else {
            (raw_title.clone(), verb.clone(), subject.clone(), false)
        };

        // Compute Multi-Factor Relevance Score
        let relevance_score = Self::calculate_relevance_score(
            cluster,
            &final_title,
            &final_verb,
            &final_subject,
            &salient_terms,
            is_specialized,
        );

        let mut item_ids_set = HashSet::new();
        let mut primary_evidence_types = HashSet::new();
        primary_evidence_types.insert(source_type);

        let mut evidence_references = Vec::new();
        for ev in cluster {
            if let Some(ref entity) = ev.source_entity {
                if entity.entity_type == EntityType::Item {
                    item_ids_set.insert(entity.entity_id.clone());
                }
            }
            primary_evidence_types.insert(ev.evidence_type);
            evidence_references.push(TaskCandidateEvidenceRef::from_task_evidence(ev));
        }

        let mut item_ids: Vec<String> = item_ids_set.into_iter().collect();
        item_ids.sort();

        let mut primary_types_vec: Vec<TaskEvidenceType> =
            primary_evidence_types.into_iter().collect();
        primary_types_vec.sort_by_key(|t| t.as_str());

        Some(ExtractedTaskCandidate {
            title: final_title.clone(),
            action_verb: final_verb,
            target_subject: final_subject,
            relevance_score,
            evidence_references,
            item_ids,
            primary_evidence_types: primary_types_vec,
            salient_terms,
            context_id: context_id.map(|s| s.to_string()),
            metadata: json!({
                "is_specialized": is_specialized,
                "is_generic": is_generic_task_name(&final_title),
                "evidence_count": cluster.len(),
            }),
        })
    }

    // =========================================================================
    // Specialized Intent Extractors
    // =========================================================================

    /// Extracts task intent from PR, issue tracker, or specific documentation URLs.
    fn extract_pr_or_issue_intent(
        cluster: &[TaskEvidence],
    ) -> Option<(String, Option<String>, Option<String>)> {
        for ev in cluster {
            if ev.evidence_type == TaskEvidenceType::UrlsDomains {
                if let Some(url_str) = ev.attributes.get("url").and_then(|v| v.as_str()) {
                    let domain = ev.attributes.get("domain").and_then(|v| v.as_str()).unwrap_or("");
                    let title_hint = ev.attributes.get("title").and_then(|v| v.as_str()).unwrap_or("");

                    // Check URL string / domain
                    if url_str.contains("github.com") && url_str.contains("/pull/") {
                        let pr_num = url_str
                            .split("/pull/")
                            .nth(1)
                            .and_then(|s| s.split('/').next())
                            .unwrap_or("");
                        let clean_title = title_hint
                            .replace("by ", "")
                            .replace(" · Pull Request #", " #")
                            .replace(" · GitHub", "");

                        let subject = if !clean_title.is_empty() && !clean_title.to_lowercase().contains("github") {
                            format!("PR #{}: {}", pr_num, clean_title.trim())
                        } else {
                            format!("PR #{} in GitHub", pr_num)
                        };

                        return Some((
                            format!("Reviewing {}", subject),
                            Some("Reviewing".to_string()),
                            Some(subject),
                        ));
                    }

                    if url_str.contains("airbnb.") || domain.contains("airbnb.") {
                        let clean_title = title_hint
                            .replace(" - Airbnb", "")
                            .trim()
                            .to_string();

                        if !clean_title.is_empty() {
                            return Some((
                                format!("Booking {}", clean_title),
                                Some("Booking".to_string()),
                                Some(clean_title),
                            ));
                        }
                    }

                    if url_str.contains("canvas.") || domain.contains("canvas.") || title_hint.contains("CS106") {
                        let clean_title = title_hint
                            .replace(" - Canvas", "")
                            .replace(" : Stanford University", "")
                            .trim()
                            .to_string();

                        if !clean_title.is_empty() {
                            return Some((
                                format!("Studying {}", clean_title),
                                Some("Studying".to_string()),
                                Some(clean_title),
                            ));
                        }
                    }
                }
            }
        }
        None
    }

    /// Extracts task intent from source code filenames and document paths.
    fn extract_filename_intent(
        cluster: &[TaskEvidence],
    ) -> Option<(String, Option<String>, Option<String>)> {
        for ev in cluster {
            if ev.evidence_type == TaskEvidenceType::Filenames {
                let name = ev
                    .attributes
                    .get("filename")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if name.is_empty() {
                    continue;
                }

                let ext = extract_file_extension(name).unwrap_or_default();
                let stem = if let Some(pos) = name.rfind('.') {
                    &name[..pos]
                } else {
                    name
                };
                let file_stem = stem.split('/').last().unwrap_or(stem);
                let human_subject = humanize_identifier(file_stem);

                if !human_subject.is_empty() && !is_generic_task_name(&human_subject) {
                    let verb = match ext.as_str() {
                        "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "cpp" | "c" | "go" | "java"
                        | "swift" => "Implementing",
                        "sql" | "prisma" => "Designing Database Schema for",
                        "md" | "docx" | "txt" | "pdf" => "Drafting",
                        "xlsx" | "csv" => "Analyzing Financial Model for",
                        "fig" | "sketch" | "psd" => "Designing UI for",
                        _ => "Working on",
                    };

                    let title = format!("{} {}", verb, human_subject);
                    return Some((title, Some(verb.to_string()), Some(human_subject)));
                }
            }
        }
        None
    }

    /// Extracts task intent from cleaned window titles and application context.
    fn extract_window_title_intent(
        cluster: &[TaskEvidence],
    ) -> Option<(String, Option<String>, Option<String>)> {
        for ev in cluster {
            if ev.evidence_type == TaskEvidenceType::ApplicationContext {
                let cleaned = ev
                    .attributes
                    .get("window_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let app = ev
                    .attributes
                    .get("app_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if cleaned.is_empty() || is_generic_task_name(cleaned) {
                    continue;
                }

                let app_lower = app.to_lowercase();
                let verb = if app_lower.contains("code")
                    || app_lower.contains("xcode")
                    || app_lower.contains("intellij")
                    || app_lower.contains("terminal")
                {
                    "Developing"
                } else if app_lower.contains("figma") || app_lower.contains("photoshop") {
                    "Designing"
                } else if app_lower.contains("notion") || app_lower.contains("docs") {
                    "Documenting"
                } else {
                    "Working on"
                };

                let target = if cleaned.contains('.') {
                    let stem = if let Some(pos) = cleaned.rfind('.') {
                        &cleaned[..pos]
                    } else {
                        cleaned
                    };
                    let file_stem = stem.split('/').last().unwrap_or(stem);
                    humanize_identifier(file_stem)
                } else {
                    cleaned.to_string()
                };

                let title = if target.starts_with("Fix ")
                    || target.starts_with("Add ")
                    || target.starts_with("Update ")
                    || target.starts_with("Implementing ")
                    || target.starts_with("Reviewing ")
                {
                    target.clone()
                } else {
                    format!("{} {}", verb, target)
                };

                return Some((title, Some(verb.to_string()), Some(target)));
            }
        }
        None
    }

    /// Extracts task intent from salient n-grams and recurring term collocations.
    fn extract_content_collocation_intent(
        cluster: &[TaskEvidence],
    ) -> Option<(String, Option<String>, Option<String>)> {
        let mut titles: Vec<String> = Vec::new();
        for ev in cluster {
            if let Some(t) = ev.attributes.get("title").and_then(|v| v.as_str()) {
                if !t.is_empty() && !is_generic_task_name(t) {
                    titles.push(t.to_string());
                }
            }
        }

        if let Some(best_title) = titles.first() {
            let cleaned = clean_window_title(best_title);
            if !is_generic_task_name(&cleaned) {
                return Some((cleaned.clone(), None, Some(cleaned)));
            }
        }

        None
    }

    // =========================================================================
    // Specialization & Scoring
    // =========================================================================

    /// Specializes a generic task title (e.g. "Coding", "Work", "Research") by combining
    /// an action verb with concrete entities (filename stems, URL slugs, or salient terms).
    fn specialize_generic_title(
        generic_title: &str,
        cluster: &[TaskEvidence],
        salient_terms: &[String],
    ) -> (String, Option<String>, Option<String>, bool) {
        // 1. Check if we have a specific file stem
        for ev in cluster {
            if ev.evidence_type == TaskEvidenceType::Filenames {
                if let Some(stem) = ev.attributes.get("filename").and_then(|v| v.as_str()) {
                    let stem_clean = if let Some(pos) = stem.rfind('.') {
                        &stem[..pos]
                    } else {
                        stem
                    };
                    let human = humanize_identifier(stem_clean);
                    if !human.is_empty() && !is_generic_task_name(&human) {
                        let verb = "Implementing".to_string();
                        let title = format!("{} {}", verb, human);
                        return (title, Some(verb), Some(human), true);
                    }
                }
            }
        }

        // 2. Check if we have a specific URL domain or path
        for ev in cluster {
            if ev.evidence_type == TaskEvidenceType::UrlsDomains {
                if let Some(title) = ev.attributes.get("title").and_then(|v| v.as_str()) {
                    let clean = clean_window_title(title);
                    if !clean.is_empty() && !is_generic_task_name(&clean) {
                        return (clean.clone(), None, Some(clean), true);
                    }
                }
                if let Some(domain) = ev.attributes.get("domain").and_then(|v| v.as_str()) {
                    if !domain.is_empty() && domain != "localhost" && !domain.contains("google.com") {
                        let brand = domain.split('.').next().unwrap_or(domain);
                        let human_brand = humanize_identifier(brand);
                        let title = format!("Researching {}", human_brand);
                        return (title, Some("Researching".to_string()), Some(human_brand), true);
                    }
                }
            }
        }

        // 3. Combine with top 2 salient terms if non-generic
        let specific_terms: Vec<String> = salient_terms
            .iter()
            .filter(|t| !is_generic_task_name(t))
            .take(2)
            .map(|t| humanize_identifier(t))
            .collect();

        if !specific_terms.is_empty() {
            let subject = specific_terms.join(" & ");
            let verb = "Working on".to_string();
            let title = format!("{} {}", verb, subject);
            return (title, Some(verb), Some(subject), true);
        }

        (generic_title.to_string(), None, None, false)
    }

    /// Calculates multi-factor initial relevance score [0.0, 1.0].
    fn calculate_relevance_score(
        cluster: &[TaskEvidence],
        title: &str,
        action_verb: &Option<String>,
        target_subject: &Option<String>,
        salient_terms: &[String],
        is_specialized: bool,
    ) -> f64 {
        let mut score = 0.50;

        let unique_types: HashSet<TaskEvidenceType> =
            cluster.iter().map(|e| e.evidence_type).collect();
        score += ((unique_types.len() as f64 - 1.0).max(0.0) * 0.05).min(0.20);

        if action_verb.is_some() && target_subject.is_some() {
            score += 0.15;
        }

        let lower_title = title.to_lowercase();
        let term_match_count = salient_terms
            .iter()
            .filter(|term| lower_title.contains(term.as_str()))
            .count();
        if term_match_count > 0 {
            score += 0.10;
        }

        let has_observed = cluster.iter().any(|e| e.provenance == EvidenceProvenance::Observed);
        if has_observed {
            score += 0.10;
        }

        if is_generic_task_name(title) {
            score -= 0.30;
        } else if is_specialized {
            score += 0.05;
        }

        score.clamp(0.05, 0.98)
    }
}

// =========================================================================
// Helper Utilities
// =========================================================================

/// Converts snake_case, kebab-case, or CamelCase identifiers into Title Case words.
pub fn humanize_identifier(ident: &str) -> String {
    let mut words = Vec::new();
    let mut current_word = String::new();

    let chars: Vec<char> = ident.chars().collect();
    for i in 0..chars.len() {
        let c = chars[i];
        if c == '_' || c == '-' || c == '.' || c == '/' {
            if !current_word.is_empty() {
                words.push(current_word.clone());
                current_word.clear();
            }
        } else if c.is_uppercase() {
            if !current_word.is_empty() {
                let prev_is_lower = i > 0 && chars[i - 1].is_lowercase();
                let next_is_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
                if prev_is_lower || (current_word.len() > 1 && next_is_lower) {
                    words.push(current_word.clone());
                    current_word.clear();
                }
            }
            current_word.push(c);
        } else {
            current_word.push(c);
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
        .into_iter()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

// =========================================================================
// Unit & Fixture Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::operations::create_context_signal;
    use crate::context_engine::transitions::{ContextReference, ContextSnapshot};
    use crate::db::operations::{create_item, create_workspace};

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_humanize_identifier() {
        assert_eq!(humanize_identifier("task_evidence_extractor"), "Task Evidence Extractor");
        assert_eq!(humanize_identifier("jwtAuthService"), "Jwt Auth Service");
        assert_eq!(humanize_identifier("cs106b_hw2_recursion"), "Cs106b Hw2 Recursion");
        assert_eq!(humanize_identifier("q3-financial-model.xlsx"), "Q3 Financial Model Xlsx");
    }

    #[test]
    fn test_anti_generic_task_name_filter() {
        assert!(is_generic_task_name("Work"));
        assert!(is_generic_task_name("Project"));
        assert!(is_generic_task_name("Coding"));
        assert!(is_generic_task_name("Research"));
        assert!(is_generic_task_name("Design"));
        assert!(is_generic_task_name("Website"));
        assert!(is_generic_task_name("General Coding"));
        assert!(is_generic_task_name("New Tab"));
        assert!(is_generic_task_name("Untitled Document"));

        assert!(!is_generic_task_name("Implementing JWT Auth Service"));
        assert!(!is_generic_task_name("CS106B Recursion Assignment"));
        assert!(!is_generic_task_name("Reviewing PR #105: Context Evidence"));
        assert!(!is_generic_task_name("Booking Paris Loft"));
    }

    #[test]
    fn test_fixture_software_engineering_pr_and_code() {
        let mut evidence_set = TaskEvidenceSet::new(Some("ws_contextsplit".to_string()), None);

        // GitHub PR URL Evidence
        let mut ev_pr = TaskEvidence::observed_url_domain(
            "https://github.com/org/contextsplit/pull/105",
            "github.com",
            Some("org/contextsplit/pull/105"),
            None,
        );
        ev_pr.attributes.insert("title".to_string(), Value::String("PR #105: Add Contextual Task Evidence".to_string()));
        evidence_set.add_evidence(ev_pr);

        // Filename Evidence
        evidence_set.add_evidence(TaskEvidence::observed_filename(
            "src/context_engine/task_extractor.rs",
            Some("rs"),
            None,
        ));

        // Application Context Evidence
        evidence_set.add_evidence(TaskEvidence::observed_app(
            "Visual Studio Code",
            Some("task_extractor.rs"),
            None,
        ));

        // Salient recurring terms
        evidence_set.add_evidence(TaskEvidence::inferred_repeated_terms(
            &["extractor".to_string(), "candidate".to_string(), "evidence".to_string()],
            "terms appeared across items",
            0.80,
        ));

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);

        assert!(!candidates.is_empty(), "Should extract task candidates");
        let top_candidate = &candidates[0];

        assert!(
            top_candidate.title.contains("PR #105") || top_candidate.title.contains("Task Extractor"),
            "Expected PR or Task Extractor title, got: {}",
            top_candidate.title
        );
        assert!(!is_generic_task_name(&top_candidate.title));
        assert!(top_candidate.relevance_score >= 0.70, "Expected high relevance score");
        assert!(!top_candidate.evidence_references.is_empty());
        assert!(top_candidate.primary_evidence_types.contains(&TaskEvidenceType::UrlsDomains));
    }

    #[test]
    fn test_fixture_academic_coursework() {
        let mut evidence_set = TaskEvidenceSet::new(Some("ws_stanford".to_string()), None);

        // Stanford Canvas Coursework URL
        let mut ev_url = TaskEvidence::observed_url_domain(
            "https://canvas.stanford.edu/courses/106/assignments/3",
            "canvas.stanford.edu",
            Some("courses/106/assignments/3"),
            None,
        );
        ev_url.attributes.insert(
            "title".to_string(),
            Value::String("CS106B Assignment 3 : Recursion and Backtracking : Stanford University".to_string()),
        );
        evidence_set.add_evidence(ev_url);

        // C++ source code file
        evidence_set.add_evidence(TaskEvidence::observed_filename(
            "recursion_warmup.cpp",
            Some("cpp"),
            None,
        ));

        // Content excerpt
        evidence_set.add_evidence(TaskEvidence::observed_item_content(
            "item_canvas",
            Some("CS106B Homework 3"),
            "In this assignment you will implement recursive backtracking algorithms for maze solving.",
            "url",
            None,
        ));

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        assert!(!candidates.is_empty());
        let top = &candidates[0];

        assert!(
            top.title.contains("CS106B Assignment 3") || top.title.contains("Recursion"),
            "Title should reference CS106B coursework: {}",
            top.title
        );
        assert!(!is_generic_task_name(&top.title));
        assert!(top.relevance_score >= 0.70);
    }

    #[test]
    fn test_fixture_trip_planning_travel_booking() {
        let mut evidence_set = TaskEvidenceSet::new(Some("ws_travel".to_string()), None);

        // Airbnb Listing URL
        let mut ev_airbnb = TaskEvidence::observed_url_domain(
            "https://www.airbnb.com/rooms/8492019",
            "airbnb.com",
            Some("rooms/8492019"),
            None,
        );
        ev_airbnb.attributes.insert(
            "title".to_string(),
            Value::String("Cozy Marais Apartment with Balcony in Paris - Airbnb".to_string()),
        );
        evidence_set.add_evidence(ev_airbnb);

        // Google Maps URL
        let mut ev_maps = TaskEvidence::observed_url_domain(
            "https://www.google.com/maps/place/Le+Marais+Paris",
            "google.com",
            Some("maps/place/Le+Marais+Paris"),
            None,
        );
        ev_maps.attributes.insert(
            "title".to_string(),
            Value::String("Le Marais, 75004 Paris, France - Google Maps".to_string()),
        );
        evidence_set.add_evidence(ev_maps);

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        assert!(!candidates.is_empty());
        let top = &candidates[0];

        assert!(
            top.title.to_lowercase().contains("paris") || top.title.to_lowercase().contains("marais"),
            "Expected travel booking title mentioning Paris/Marais, got: {}",
            top.title
        );
        assert!(!is_generic_task_name(&top.title));
    }

    #[test]
    fn test_specialization_of_generic_input() {
        let mut evidence_set = TaskEvidenceSet::new(Some("ws_backend".to_string()), None);

        // Generic app title
        evidence_set.add_evidence(TaskEvidence::observed_app(
            "Visual Studio Code",
            Some("Coding"),
            None,
        ));

        // Specific file stem
        evidence_set.add_evidence(TaskEvidence::observed_filename(
            "jwt_token_verifier.rs",
            Some("rs"),
            None,
        ));

        let candidates = TaskCandidateExtractor::extract_from_evidence(&evidence_set, None);
        assert!(!candidates.is_empty());
        let top = &candidates[0];

        // Generic "Coding" must be specialized into "Implementing Jwt Token Verifier"
        assert_eq!(top.title, "Implementing Jwt Token Verifier");
        assert_eq!(top.action_verb.as_deref(), Some("Implementing"));
        assert_eq!(top.target_subject.as_deref(), Some("Jwt Token Verifier"));
        assert!(!is_generic_task_name(&top.title));
    }

    #[test]
    fn test_extract_from_segment_integration() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "ContextSplit App").unwrap();

        let item1 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "url",
            "https://github.com/ContextSplit/core/pull/42",
            Some("PR #42: Add Transition Stabilizer by author · GitHub"),
            r#"{"domain":"github.com"}"#,
        )
        .unwrap();

        let item2 = create_item(
            &conn,
            Some(&ws.id),
            None,
            "file",
            "src/transitions.rs",
            Some("transitions.rs"),
            r#"{"app_name":"Visual Studio Code"}"#,
        )
        .unwrap();

        // Create context signals
        create_context_signal(&conn, Some(&ws.id), Some(&item1.id), "browser_domain", None, "github.com").unwrap();
        create_context_signal(&conn, Some(&ws.id), Some(&item2.id), "file_path", None, "src/transitions.rs").unwrap();

        let segment = ContextActivitySegment {
            id: "seg_123".to_string(),
            context: Some(ContextReference::workspace(&ws.id, &ws.name)),
            started_at: "2026-09-13T01:00:00Z".to_string(),
            ended_at: "2026-09-13T01:30:00Z".to_string(),
            duration_seconds: 1800,
            duration_formatted: "30m".to_string(),
            confidence: 0.90,
            session_ids: vec![],
            item_references: vec![
                SegmentItemReference {
                    item_id: item1.id.clone(),
                    title: item1.title.clone(),
                    item_type: item1.item_type.clone(),
                    timestamp: "2026-09-13T01:05:00Z".to_string(),
                    confidence: 0.90,
                },
                SegmentItemReference {
                    item_id: item2.id.clone(),
                    title: item2.title.clone(),
                    item_type: item2.item_type.clone(),
                    timestamp: "2026-09-13T01:20:00Z".to_string(),
                    confidence: 0.90,
                },
            ],
            transition_in: None,
            transition_out: None,
            is_ambiguous: false,
            item_count: 2,
            metadata: json!({}),
        };

        let candidates = TaskCandidateExtractor::extract_from_segment(&conn, &segment, None).unwrap();
        assert!(!candidates.is_empty(), "Should extract candidate from segment");
        let top = &candidates[0];
        assert!(!is_generic_task_name(&top.title));
        assert!(top.relevance_score >= 0.50);
    }

    #[test]
    fn test_extract_from_memory_snapshot() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Bio Research").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "CRISPR Cas9 Gene Editing Protocol and Analysis",
            Some("CRISPR Cas9 Protocol"),
            r#"{"app_name":"Notion"}"#,
        )
        .unwrap();

        create_context_signal(&conn, Some(&ws.id), Some(&item.id), "app_name", None, "Notion").unwrap();

        let snapshot = ContextSnapshot {
            reference: ContextReference::workspace(&ws.id, &ws.name),
            confidence: 0.88,
            confidence_band: "high".to_string(),
            first_seen_at: "2026-09-13T00:00:00Z".to_string(),
            last_seen_at: "2026-09-13T01:00:00Z".to_string(),
            streak_count: 5,
            session_id: None,
        };

        let item_refs = vec![SegmentItemReference {
            item_id: item.id.clone(),
            title: item.title.clone(),
            item_type: item.item_type.clone(),
            timestamp: "2026-09-13T00:30:00Z".to_string(),
            confidence: 0.88,
        }];

        let candidates =
            TaskCandidateExtractor::extract_from_memory(&conn, &snapshot, Some(&item_refs), None)
                .unwrap();
        assert!(!candidates.is_empty());
        let top = &candidates[0];
        assert!(!is_generic_task_name(&top.title));
    }

    #[test]
    fn test_extract_from_context_memory() {
        let conn = setup_test_db();
        let ws = create_workspace(&conn, "Portfolio").unwrap();

        let item = create_item(
            &conn,
            Some(&ws.id),
            None,
            "text",
            "Hero animation interactive spring physics implementation in Framer Motion",
            Some("Hero Animation Physics"),
            r#"{"app_name":"Figma"}"#,
        )
        .unwrap();

        create_context_signal(&conn, Some(&ws.id), Some(&item.id), "app_name", None, "Figma").unwrap();

        let ctx = ContextReference::workspace(&ws.id, &ws.name);
        let memory = ContextMemory::new(Some(ctx), 0.95, "2026-09-13T10:00:00Z");

        let item_refs = vec![SegmentItemReference {
            item_id: item.id.clone(),
            title: item.title.clone(),
            item_type: item.item_type.clone(),
            timestamp: "2026-09-13T10:00:00Z".to_string(),
            confidence: 0.95,
        }];

        let candidates = TaskCandidateExtractor::extract_from_context_memory(
            &conn,
            &memory,
            Some(&item_refs),
            None,
        )
        .unwrap();

        assert!(!candidates.is_empty());
        let top = &candidates[0];
        assert!(!is_generic_task_name(&top.title));
        assert_eq!(memory.context_name(), "Portfolio");
    }
}
