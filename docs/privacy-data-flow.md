# ContextSplit Privacy & Contextual Data Flow Audit

**Audit Date**: September 24, 2026  
**Document**: `docs/privacy-data-flow.md`  
**System Evaluated**: ContextSplit Core Engine, Context Observability, Diagnostics, & Storage Layers  
**Security & Privacy Classification**: Local-First, Zero-Cloud Telemetry, End-to-End Audited  

---

## 1. Executive Summary & Foundational Privacy Axioms

ContextSplit is architected from the ground up as a **strictly local-first, privacy-preserving desktop context intelligence engine**. ContextSplit never acts as an unrestricted ambient surveillance tool or data vacuum. All contextual inference, activity segmentation, task extraction, state reconstruction, performance tracing, and developer diagnostics operate entirely on the user's local machine with **zero external cloud data transmission**.

### Core Privacy Axioms:
1. **Local-First Invariance**: No captured telemetry, contextual evidence, episodic memory, decision traces, or diagnostics are ever transmitted to external servers, cloud databases, or third-party analytics services by default.
2. **Strict Sensitive Token Scrubbing**: All incoming activity strings, titles, URLs, and file paths are pre-filtered through automated regex scrubbing pipelines (`scrub_private_data`) to purge passwords, authentication tokens, API keys, bearer tokens, JWTs, AWS credentials, and private cryptographic keys before any database storage or memory synthesis.
3. **Data Minimization & Structural Referencing**: ContextSplit stores lightweight entity references, structural identifiers, and bounded metadata rather than arbitrary file content payloads, screenshot image buffers, or raw keystroke streams.
4. **Complete Diagnostics Opt-in & Zero Production Leakage**: Development diagnostics (decision traces, debug timelines, internal confidence ratings, performance spans, and cross-system consistency checks) are completely disabled by default in production mode, are strictly memory-bounded when enabled, and can be instantly purged at any time.
5. **Full User Sovereignty & Deletability**: All contextual workspaces, sessions, tasks, snapshots, memories, and feedback events are fully inspectable, editable, and deletable by the user.

---

## 2. Comprehensive Contextual Data Flow Matrix

The following matrix categorizes every piece of data captured, processed, stored, and derived by ContextSplit:

| Data Category | Specific Data Elements | Why It Is Collected | Where It Is Stored | Retention Policy | Required? | User Deletable? | Externally Exposed? |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Captured Data** | Active window titles, foreground app names, browser domain/URL paths, active project/file paths, focus transition timestamps. | Required to ground user work in discrete projects and detect context transitions. | SQLite `items` table; in-memory capture queue. | Retained locally until user deletes workspace or purges history. | **Yes** (core capture). | **Yes** (item / workspace deletion). | **No** (strictly local). |
| **Stored Data** | Workspaces (`workspaces`), items (`items`), item memberships (`context_memberships`), sessions (`sessions`), session items (`session_items`), tasks (`tasks`), snapshots (`context_snapshots`), lifecycle events (`context_lifecycle_events`), feedback records (`context_feedback_events`), important memories (`context_important_memories`). | Maintains long-term workspace definitions, project relationships, active task states, and continuity across desktop sessions. | Local SQLite database file (`contextsplit.db`). | Persistent until explicitly deleted, archived, or purged by user. | **Yes** (core data model). | **Yes** (per-entity and complete workspace wipe). | **No** (strictly local). |
| **Derived Data** | Context candidates, normalized evidence vectors, activity segments, active worksets (`ContextWorkset`), state assessments (`ContextState`), resume readiness metrics, context reconstructions (`ContextReconstruction`). | Synthesizes high-level project awareness, recovers active task context, and answers "Where did I leave off?". | Computed deterministically in-memory; bounded active state persisted to SQLite tables (`context_snapshots`, `context_important_memories`). | Dynamic in-memory lifecycle; point-in-time snapshots retained per workspace cap. | **Yes** (engine core). | **Yes** (cleared on context deletion/reset). | **No** (strictly local). |
| **Temporary Data** | In-memory trace buffers (`ContextDecisionTracer`), performance span ring buffers (`PerformanceTracer`), pending activity queue (`PendingActivityManager`), reconstruction caches (`ReconstructionCache`). | Buffers transient events for hysteresis evaluation, debounce filters, and rapid reconstruction without repeated database scans. | Ephemeral RAM only (thread-safe ring buffers and RwLocks). | Cleared on application shutdown, context switch, or manual purge. | **No** (transient cache). | **Yes** (automatic or via `purge_all_diagnostics`). | **No** (strictly local). |
| **Diagnostic Data** | Decision traces, lifecycle transition explanations, memory provenance explanations, feedback effect explanations, health check reports, performance traces, cross-system consistency matrices. | Developer explainability, diagnostic inspection, and pipeline performance optimization. | Ephemeral RAM ring buffer (capped at 500–1,000 records); optional debug logging when enabled. | Ephemeral; discarded on shutdown or instant purge. Zero storage in production mode. | **No** (development opt-in only). | **Yes** (instant purge via `DiagnosticsController`). | **No** (strictly local). |
| **Feedback Data** | Explicit user confirmations, rejections, workspace renames, task corrections, item removals from context, feedback scope (`item_context`, `task_context`, `global_rule`). | Adapts intelligence heuristics deterministically to respect user explicit corrections and override inferred decisions. | SQLite `context_feedback_events` table. | Persistent to maintain user preferences across app launches. | **Optional** (created on user action). | **Yes** (user can reset feedback rules). | **No** (strictly local). |
| **Memory Data** | Episodic memory anchors, important facts, recurring project tools, active task associations, importance scores, provenance tags (`Confirmed`, `Inferred`, `Observed`). | Accelerates context restoration and grounds cognitive resume state when switching back to previous projects. | SQLite `context_important_memories` table and memory cache. | Decays over time based on non-usage unless explicitly marked important; persistent until deleted. | **Yes** (for memory feature). | **Yes** (user can remove any memory). | **No** (strictly local). |

---

## 3. Sensitive Data Protection & Scrubbing Verification

ContextSplit enforces automated regex-based credential and token scrubbing across all capture and storage boundaries:

### 3.1 Sanitization Filters (`scrub_private_data`)
Before any window title, URL, file path, task title, content snippet, or diagnostic trace is stored or displayed, it is evaluated against the following scrubbing rules:

1. **Passwords & Secrets**: Matches and replaces query parameters, form fields, and CLI flags containing `password=`, `pwd=`, `secret=`, `token=`, `auth=`.
2. **API Keys & Bearer Tokens**:
   - `Bearer [A-Za-z0-9\-\._~\+\/]+=*` $\rightarrow$ `Bearer [REDACTED]`
   - `sk_live_[0-9a-zA-Z]{24,}` $\rightarrow$ `[REDACTED_API_KEY]`
   - `ghp_[0-9a-zA-Z]{36}` (GitHub Personal Access Tokens) $\rightarrow$ `[REDACTED_TOKEN]`
   - `gho_[0-9a-zA-Z]{36}` (GitHub OAuth Tokens) $\rightarrow$ `[REDACTED_TOKEN]`
   - `glpat-[0-9a-zA-Z\-_]{20,}` (GitLab Personal Access Tokens) $\rightarrow$ `[REDACTED_TOKEN]`
   - `AKIA[0-9A-Z]{16}` (AWS Access Key ID) $\rightarrow$ `[REDACTED_AWS_KEY]`
   - `aws_secret_access_key=[^\s&]+` $\rightarrow$ `[REDACTED_AWS_SECRET]`
3. **JSON Web Tokens (JWT)**:
   - `eyJ[A-Za-z0-9-_=]+\.[A-Za-z0-9-_=]+\.?[A-Za-z0-9-_.+/=]*` $\rightarrow$ `[REDACTED_JWT]`
4. **Private Cryptographic Keys**:
   - `-----BEGIN [A-Z ]+ PRIVATE KEY-----[\s\S]*?-----END [A-Z ]+ PRIVATE KEY-----` $\rightarrow$ `[REDACTED_PRIVATE_KEY]`
5. **Database Connection Strings**:
   - `postgres://[^\s@]+@[^\s/]+/[^\s]+` $\rightarrow$ `postgres://[REDACTED]@...`
   - `mongodb(\+srv)?://[^\s@]+@[^\s/]+/[^\s]+` $\rightarrow$ `mongodb://[REDACTED]@...`

### 3.2 Audit of Sensitive Data in Logs & Traces
An exhaustive code inspection confirmed that no internal logging statements (`println!`, `eprintln!`, `log::debug!`, `log::info!`, `log::warn!`, `log::error!`) print:
- Raw user clipboard text
- Raw keystroke data
- Unsanitized authentication credentials
- Full document file bodies
- Private cryptographic tokens

---

## 4. Development Diagnostics Privacy & Safety Controls

### 4.1 Production Disablement & Zero-Leakage Guarantee
The `DiagnosticsController` (`src-tauri/src/context_engine/diagnostics_guard.rs`) enforces strict runtime policy controls:

```rust
pub struct DiagnosticsPolicy {
    pub enabled: bool,                     // Production default: false
    pub expose_decision_traces: bool,      // Production default: false
    pub expose_debug_timelines: bool,      // Production default: false
    pub expose_raw_evidence: bool,         // Permanently false (even in local dev)
    pub expose_internal_confidence: bool,  // Production default: false
    pub enable_performance_tracing: bool,  // Production default: false
    pub enable_health_checks: bool,        // Production default: false
    pub enable_cross_system_checks: bool,  // Production default: false
    pub max_in_memory_records: usize,      // Production default: 0
    pub allow_external_export: bool,       // Permanently false
}
```

### 4.2 Privacy Guarantees for Diagnostics
1. **Zero Production Exposure**: In production mode (`DiagnosticsPolicy::production()`), all diagnostic collectors are immediate no-ops. Zero traces or spans are stored in memory or written to disk.
2. **Structural Hashing Over Raw Text**: Performance spans and decision traces utilize SHA-256 structural fingerprints of entity IDs (`sha256:...`) to detect N+1 queries, unnecessary recomputations, and repeated semantic processing without recording raw sensitive strings.
3. **Bounded Memory Ring Buffers**: When opt-in local development diagnostics are active (`DiagnosticsPolicy::local_dev()`), records are stored in memory-capped ring buffers (500 traces / 1,000 performance spans). Old records are automatically evicted using FIFO rotation to prevent memory leaks or unbounded accumulation.
4. **Instant On-Demand Purge**: Developers or users can invoke `purge_all_diagnostics()` at any time to immediately clear all traces, performance metrics, and debug caches from RAM.
5. **Export Lock**: `allow_external_export` is permanently guarded to ensure diagnostic data cannot be dispatched across network sockets.

---

## 5. Security & Privacy Audit Verification Checklist

| Audit Checkpoint | Verified State | Evidence & Implementation Reference |
| :--- | :---: | :--- |
| **Local-First Architecture** | **PASS** | SQLite database stored locally in user application support directory. Zero network dispatch code in contextual intelligence layers. |
| **Sensitive Token Scrubbing** | **PASS** | `scrub_private_data` active in `evidence.rs`, `memory.rs`, `workset.rs`, `snapshot.rs`, and `decision_trace.rs`. |
| **No Raw Keystrokes / Screen Buffers** | **PASS** | Capture architecture records only high-level window title and process name metadata. No keylogging or screen recording storage. |
| **Diagnostics Opt-In Guard** | **PASS** | `DiagnosticsController::is_diagnostics_enabled()` and `DiagnosticsPolicy::production()` enforce complete disablement in release mode. |
| **Bounded Diagnostic Buffers** | **PASS** | `PerformanceTracer::new(1000)` and `ContextDecisionTracer` bounded to ring buffers with automatic FIFO eviction. |
| **Instant Diagnostic Purge** | **PASS** | `DiagnosticsController::purge_all_diagnostics()` cleanly wipes memory. |
| **User Data Deletability** | **PASS** | SQLite cascade deletions and explicit feedback APIs allow complete removal of workspaces, items, tasks, and memories. |
| **Zero External Diagnostics Dispatch** | **PASS** | `allow_external_export: false` enforced in all policy variants. |

---

## 6. Conclusion

ContextSplit's contextual data flow and observability architecture fully comply with modern privacy standards, zero-trust local execution, and strict data minimization. Development diagnostics provide developer observability without compromising user privacy, leaking sensitive credentials, or violating local-first invariants.
