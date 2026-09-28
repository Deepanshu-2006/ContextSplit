# ContextSplit Phase 1 — Data Safety and Privacy Audit

**Audit Date:** September 29, 2026  
**Document:** `docs/data-safety.md`  
**Scope:** Activity Collection, Clipboard Monitoring, Diagnostic Systems, Database Records, Logs, and Decision Traces  
**Classification:** Local-First, Zero-Telemetry, Sensitive Credential Excluded  

---

## 1. Executive Summary & Core Data Safety Invariants

ContextSplit operates as a continuous background assistant designed to assist users in managing workflows, contexts, and task states across desktop applications. Because the application remains active throughout the day, strict data safety and privacy boundaries are required to guarantee that no private, sensitive, or unintended data is captured or retained.

### Core Data Safety Guarantees:
1. **Zero Cloud Telemetry / Local-Only Persistence**: All data is stored in a local SQLite database (`contextsplit.db`) in the user's application data directory. No activity events, context models, decision traces, or telemetry are ever transmitted to external cloud servers.
2. **Pre-Persistence Sensitive Content Rejection**: High-entropy passwords, API keys, bearer tokens, JWTs, SSH private keys, 2FA/OTP codes, and database connection strings are detected and rejected *before* database persistence occurs.
3. **Excluded Applications & Password Managers**: Known credential managers (1Password, Bitwarden, KeePass, LastPass, Dashlane, Keychain Access, Google Authenticator, etc.) are excluded by default.
4. **Metadata & References Over Raw Content**: Wherever possible, ContextSplit stores structural metadata (e.g. application names, domain names, file paths, MIME types, character counts) rather than raw document bodies or screen buffers.
5. **No Keylogging or Screen Recording**: ContextSplit captures window-level focus transitions and clipboard changes (when permitted); it never records raw keystrokes, mouse coordinate streams, or full desktop screenshots.
6. **Zero Production Diagnostic Exposure**: Development diagnostics (decision traces, performance spans, heuristic scores) are completely disabled by default in production and bounded to memory-capped ring buffers during local development.

---

## 2. Complete Inventory of Stored Data

Every table and field in the local SQLite database schema was audited to verify exactly what data is retained:

| Table Name | Primary Purpose | Fields Stored | Sensitive Content Risk | Safety Controls Applied |
| :--- | :--- | :--- | :---: | :--- |
| `workspaces` | Context / Project definitions | `id`, `name`, `created_at`, `updated_at`, `lifecycle_state`, `last_activity_at`, `archived_at` | Low | User-defined or inferred project names; no credential fields. |
| `items` | Captured contextual elements (text, URLs, files, images) | `id`, `workspace_id`, `item_type`, `content`, `title`, `metadata` (JSON), `created_at`, `updated_at`, `session_id` | Medium | Pre-filtered through `ClipboardPrivacyFilter` and `scrub_private_data`; rejected if credential detected. |
| `context_signals` | High-level telemetry signals | `id`, `workspace_id`, `item_id`, `signal_type`, `session_id`, `payload` (JSON), `created_at`, `updated_at` | Low | Stores structured metadata (`app_name`, `domain`, `event_type`); gated by user privacy toggles. |
| `sessions` | Desktop engagement intervals | `id`, `started_at`, `last_activity_at`, `ended_at`, `status` | Low | Only UTC timestamps and status strings; zero raw content. |
| `item_relationships`| Semantic & co-occurrence graph | `id`, `item_a_id`, `item_b_id`, `relationship_type`, `strength`, `metadata`, `created_at` | Low | Stores relational IDs, relationship types, and scalar weights ($0.0 \dots 1.0$). |
| `settings` | User configuration | Toggles for active app, window title, browser domain, semantic processing, AI reasoning, capture pause | Low | Local configuration booleans and provider preference strings. |
| `item_embeddings` | Vector embeddings for similarity | `item_id`, `embedding` (BLOB), `provider`, `model`, `created_at` | Low | Numerical vector arrays (dense floats); non-invertible to raw text. |
| `context_candidates`| Discovered context clusters | `id`, `status`, `confidence`, `label`, `metadata`, `created_at`, `updated_at` | Low | Synthetic cluster labels and scalar confidence metrics. |
| `context_memberships`| Item-to-context mappings | `id`, `item_id`, `workspace_id`, `membership_type`, `confidence`, `source`, `created_at`, `updated_at` | Low | Foreign key references and classification confidence numbers. |
| `context_transitions`| Project switch audit trail | `id`, `from_context_id`, `to_context_id`, `transition_type`, `confidence`, `detected_at`, `session_id`, `trigger_item_id`, `metadata` | Low | Relational references and switch timestamps. |
| `task_candidates` | Extracted user tasks | `id`, `context_id`, `title`, `confidence`, `status`, `first_detected_at`, `last_active_at` | Low | Normalized task titles scrubbed of private tokens. |
| `context_snapshots` | Point-in-time state captures | `id`, `context_id`, `context_name`, `task_id`, `task_title`, `state`, `snapshot_confidence`, `quality_rating`, `snapshot_json` | Low | Bounded JSON representations of recent worksets and memory references. |
| `ai_reasoning_cache`| Local cache for AI inference | `package_hash` (SHA-256), `item_id`, `user_id`, `result_json`, `created_at`, `expires_at` | Low | SHA-256 hashed queries with 7-day automatic TTL expiration. |
| `inbox_item_states` | Lifecycle state of unfiled items | `item_id`, `state`, `previous_state`, `reason`, `created_at`, `updated_at` | Low | State transition strings (`new`, `triaged`, `dismissed`, `promoted`). |

---

## 3. Specific Data Verification Audits

### 3.1 Passwords, Authentication Tokens, and API Keys
- **Detection Engine:** Implemented in `src-tauri/src/capture/privacy.rs` (`detect_sensitive_content`).
- **Patterns Evaluated:**
  - **AWS Access Keys:** `\b(AKIA|ABIA|ACCA|ASIA)[0-9A-Z]{16}\b`
  - **OpenAI Keys:** `sk-[A-Za-z0-9_-]{20,}`
  - **GitHub Tokens:** `(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}`
  - **GitLab PATs:** `glpat-[A-Za-z0-9_-]{20,}`
  - **Google API Keys:** `AIza[0-9A-Za-z_-]{35}`
  - **Slack Tokens:** `xox[baprs]-[A-Za-z0-9_-]{10,}`
  - **JWTs:** Standard header.payload.signature pattern detection
  - **Bearer Tokens:** `\bBearer\s+[A-Za-z0-9\-\._~+/]{20,}`
  - **Explicit Passwords:** Key-value pairs matching `password=`, `pwd=`, `db_pass=`, and high-complexity unspaced strings
- **Action on Detection:** Capture is immediately aborted with `PrivacyFilterResult::Rejected(SensitiveContent)`. No database row or signal is written.

### 3.2 Private Cryptographic Keys & Connection Strings
- **Private Keys:** Evaluates for `BEGIN ... PRIVATE KEY` blocks (RSA, EC, OpenSSH, DSA). Completely rejected before normalization.
- **Connection Strings & Basic Auth:** Evaluates for embedded database connection strings (`postgres://user:pass@host/db`, `mysql://...`, `mongodb://...`, `redis://...`) and inline URL basic authentication (`http://user:pass@domain`).

### 3.3 Clipboard Contents & Ephemeral Noise
- **De-noising & De-duplication:** Ephemeral clipboard items (e.g. rapid copy-paste sequences) are deduplicated within a 3-second window using SHA-256 content hashing.
- **Exclusion of Password Managers:** 1Password, Bitwarden, KeePassXC, Dashlane, LastPass, Keychain Access, and Enpass are automatically detected by source process name or window title and excluded unconditionally.
- **Global & Temporary Pause:** Users can pause clipboard monitoring indefinitely or for a timed duration (e.g. 15 minutes), during which zero clipboard captures are read or persisted.

### 3.4 Browsing Content & URLs
- **Domain-Level Preference:** When web activity is tracked, the system extracts the hostname/domain and top-level path rather than deep query strings that might hold session tokens or OAuth state.
- **Query Parameter Scrubbing:** Query strings containing `token=`, `auth=`, `session=`, `key=`, or `secret=` are scrubbed by `scrub_private_data`.

---

## 4. Review of Subsystems

### 4.1 Logs & Standard Output
- Verified that `println!` and `eprintln!` statements across production code paths do not output user clipboard text, document contents, or raw tokens.
- Error logs in `clipboard/mod.rs` only log high-level OS error descriptions (`Clipboard monitor error: ...`), never the buffer content.

### 4.2 Diagnostics & Performance Tracing
- **Production Guard:** `DiagnosticsPolicy::production()` sets `enabled = false` and `max_in_memory_records = 0`. All diagnostic collectors are immediate no-ops in release builds.
- **Raw Evidence Redaction:** `DiagnosticsPolicy` sets `expose_raw_evidence = false` even in local development mode. Traces record entity IDs and structural hashes instead of raw document strings.
- **Purge Support:** `DiagnosticsController::purge_all_diagnostics()` provides an instant RAM wipe of all active spans and decision traces.
- **Zero Remote Export:** `allow_external_export` is hardcoded to `false`.

### 4.3 Error Reports & Decision Traces
- Decision traces (`ContextDecisionTracer`) record explainable classification logic (e.g. `"Window title contains project keyword 'payx' -> assigned with confidence 0.92"`).
- Explanations never include unscrubbed secrets or raw clipboard buffers.

---

## 5. Verification Checklist & Audit Summary

| Audit Checkpoint | Status | Verification Detail |
| :--- | :---: | :--- |
| **No Unintentional Password Storage** | **VERIFIED** | Multi-pattern regex and entropy filters reject credentials before insertion. |
| **No API Key / Token Storage** | **VERIFIED** | AWS, GitHub, OpenAI, Google, Slack, JWT, and Bearer patterns rejected. |
| **No Private Cryptographic Key Storage** | **VERIFIED** | PEM and SSH private key blocks rejected. |
| **Password Manager Process Exclusions** | **VERIFIED** | 1Password, Bitwarden, KeePass, and Keychain excluded by default. |
| **Local-First Isolation** | **VERIFIED** | 100% SQLite local file storage; zero external telemetry or cloud sync. |
| **Diagnostics Production Safe** | **VERIFIED** | Disabled by default in production; memory-bounded in development; zero external export. |
| **User Data Erasure** | **VERIFIED** | Cascade deletions supported on all entities; user has complete sovereignty over database. |

### Conclusion
ContextSplit Phase 1 activity collection and diagnostic subsystems adhere strictly to zero-trust local-first data safety standards. No unverified or unsafe data handling exists in the codebase.
