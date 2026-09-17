# Context Memory Privacy Audit

**Audit Date**: September 18, 2026  
**System Evaluated**: ContextSplit Context Memory & Knowledge Synthesis Architecture  
**Scope**: `ContextMemory`, `ContextWorkset`, `ContextState`, `ContextSnapshot`, `ImportantMemory`, `ContextEvidence`, `TaskCandidate`, and `User Feedback`  
**Status**: Passed with Active Verification  

---

## 1. Executive Summary & Guarantees

ContextSplit's contextual memory system derives high-level working awareness from local user interactions without acting as an unrestricted data vacuum. The Context Memory system is designed with strict data minimization principles:

1. **Capture Privacy Boundary Invariance**: Memory processing never bypasses capture exclusions. If an item or application was excluded at the capture boundary, it never enters database persistence, working memory, or semantic graphs.
2. **Zero Sensitive Token Retention**: Passwords, API keys, bearer tokens, JWTs, AWS credentials, database connection strings, and PEM private keys are actively scrubbed by regex filters (`scrub_private_data`) before entering memory representations, titles, snippets, or evidence objects.
3. **No Unbounded Memory Dumps**: Full clipboard dumps, raw keystroke streams, screenshot bitmaps, and arbitrary file contents are never copied into memory structures. Context memory uses lightweight, bounded entity references (`MemoryItemReference`, `SnapshotWorksetItemReference`, `ContextImportantMemory`).
4. **Zero Hidden Application Telemetry**: Memory records only explicit foreground context necessary for task grounding (sanitized window titles, application names, domain slugs). Unnecessary telemetry or background OS noise is actively filtered.
5. **Privacy-Preserving User Controls**: Explicit user actions (`keep_important`, `remove_important`, `remove_from_context`, `confirm_membership`, `dismiss_task`) record discrete control signals without logging sensitive payload texts.

---

## 2. Component-by-Component Privacy Inspection

### 2.1 ContextMemory (`src/context_engine/memory.rs`)
- **Structure & Purpose**: Maintains active context identity, current inferred task, and bounded chronological milestone history.
- **Payload Minimization**: Does not store full item texts or vector embeddings. Instead, maintains `recent_items: Vec<MemoryItemReference>` with a strict 20-item cap and bounded display titles (<= 60 characters).
- **Privacy Safeguards**:
  - `display_title` generation invokes `scrub_private_data(...)` on fallback content snippets and titles.
  - Epistemic provenance is strictly tagged (`Observed`, `Inferred`, `Confirmed`).
  - Explanation strings and why-reasons only reference sanitized entity IDs and titles.

### 2.2 ContextWorkset (`src/context_engine/workset.rs`)
- **Structure & Purpose**: Derived small working set (5–8 items) providing high-signal context for continuing active tasks.
- **Payload Minimization**: Stores lightweight `WorksetItemReference` objects rather than full file payloads or raw text.
- **Privacy Safeguards**:
  - `create_content_snippet` sanitizes content with `scrub_private_data` and caps length to <= 120 characters with ellipsis.
  - `is_accidental_capture(...)` suppresses empty, corrupt, or transient captures (e.g. `about:blank`, `chrome://newtab`, empty 1-2 char clipboard blips).
  - `is_generic_noise(...)` eliminates generic terms and search query artifacts without context grounding.
  - Excluded or user-dismissed items are strictly omitted.

### 2.3 ContextState (`src/context_engine/context_state.rs`)
- **Structure & Purpose**: Conservative classification of working state (`Active`, `RecentlyActive`, `Paused`, `Dormant`, `Uncertain`, `InsufficientEvidence`) and freshness.
- **Payload Minimization**: Encapsulates state metadata and structured `MeaningfulActivity` indicators.
- **Privacy Safeguards**:
  - Never stores raw keystroke streams or screen captures.
  - State descriptions use human-readable templates (e.g., "Organized Tokyo itinerary draft in TripWise").
  - Freshness wording is defensible and non-arbitrary (e.g. "Active recently", "Last active yesterday"), avoiding intrusive tracking claims.

### 2.4 ContextSnapshot (`src/context_engine/snapshot.rs`)
- **Structure & Purpose**: Point-in-time contextual resumption archive for returning to previous tasks.
- **Payload Minimization**: Copies zero full database rows or blobs. Stores references: `workset_item_references`, `recent_task_references`, `important_memory_references`.
- **Privacy Safeguards**:
  - `SnapshotWorksetItemReference::from_workset_item` scrubs titles, snippets, and reasons.
  - `SnapshotTaskReference::from_memory_task` scrubs task titles, verbs, and subjects.
  - `SnapshotActivityReference::from_memory_activity` scrubs activity descriptions.
  - `SnapshotImportantMemoryReference::from_important_memory` scrubs titles, details, and summaries.

### 2.5 ImportantMemory (`src/context_engine/important_memory.rs`)
- **Structure & Purpose**: Long-lived contextual anchors (Confirmed Facts, Active Tasks, Recurring References, Important Items, Meaningful State).
- **Payload Minimization**: Stores `ContextImportantMemory` records containing entity references, calibrated importance scores, and why-explanations.
- **Privacy Safeguards**:
  - `create_snippet` applies `scrub_private_data` before bounded 100-character truncation.
  - `title` generation scrubs credentials and tokens.
  - `ContextMemoryQualityGuard` rejects generic noise, ungrounded single matches, and items rejected by user feedback.
  - Does not duplicate raw database rows.

### 2.6 ContextEvidence (`src/context_engine/task_evidence.rs`, `evidence.rs`)
- **Structure & Purpose**: Multi-modal evidence packages supporting task extraction and contextual clustering.
- **Payload Minimization**: Limits text snippets to `max_snippet_length = 150` characters.
- **Privacy Safeguards**:
  - `TaskEvidence::observed_item_content` applies `scrub_private_data` on snippets and titles.
  - `clean_window_title` removes application suffixes and scrubs credentials from window titles.
  - `AIPrivacyBoundary` audits and sanitizes all evidence packages before any external or AI processing, strictly gating application metadata, domains, and session tracking based on user privacy settings.

### 2.7 TaskCandidate (`src/context_engine/task_extractor.rs`, `task_ranking.rs`)
- **Structure & Purpose**: Actionable task candidate clusters extracted from recent activity.
- **Payload Minimization**: Stores action verbs, target subjects, confidence scores, and evidence references.
- **Privacy Safeguards**:
  - Filters out generic task noise using `GENERIC_TASK_TERMS` and `is_generic_task_name`.
  - Cleans and strips token patterns and credentials from inferred task titles.

### 2.8 User Feedback System (`src/context_engine/important_memory.rs`, `db/operations.rs`)
- **Structure & Purpose**: Captures explicit user control actions (`keep_important`, `remove_important`, `remove_from_context`).
- **Payload Minimization**: Records only discrete control signals (`signal_type`, `item_id`, `context_id`, timestamp).
- **Privacy Safeguards**:
  - Never logs secret payloads or clipboard text in feedback logs.
  - "Remove from context" / "Forget association" severs contextual association without leaking item contents.
  - User decisions strictly override inferred importance algorithms.

---

## 3. Capture Privacy Boundary Alignment

The capture layer (`src/capture/privacy.rs`) enforces strict gatekeeping BEFORE database persistence:

```
[System Input (Clipboard/Window/File)]
                │
                ▼
   ┌───────────────────────────┐
   │  ClipboardPrivacyFilter   │
   │  - Excluded Apps          │  (1Password, Bitwarden, KeePass, Keychain, Authy, etc.)
   │  - Sensitive Content      │  (Passwords, OTPs, API keys, Private keys, Cards, SSNs, DB URIs)
   │  - Paused / Disabled      │
   └────────────┬──────────────┘
                │
        [Filter Result]
         /            \
     Rejected       Allowed
       │               │
       ▼               ▼
[Dropped / Audit]  [SQLite Persistence]
                       │
                       ▼
          ┌───────────────────────────┐
          │   Context Memory Engine   │
          │   - Content Scrubbing     │  (scrub_private_data)
          │   - Snippet Truncation    │  (100-150 chars max)
          │   - Reference-Only Store  │  (No raw dumps)
          │   - Quality Guard Filters │  (Accidental & Noise exclusion)
          └────────────┬──────────────┘
                       │
                       ▼
          [ContextMemory / Workset / Snapshot / ImportantMemory]
```

### Invariants:
1. **No Capture Boundary Bypass**: If an event is rejected by `ClipboardPrivacyFilter`, it is dropped immediately and never reaches SQLite or Context Memory.
2. **Double Defense**: Even if sensitive data is imported or present in unclassified text, Context Memory runs secondary `scrub_private_data` sanitization on all snippets and titles.
3. **No Raw Keystrokes or Screenshots**: Memory structures contain zero fields for raw keystroke buffers or image bitmaps.

---

## 4. Privacy Filter Rules Matrix

| Sensitivity Category | Detection & Scrubbing Pattern | Handled In Memory |
|:---|:---|:---:|
| **Passwords / Passphrases** | `(?i)\b(password\|passwd\|secret\|apikey)[ \t]*[:=]...` | ✅ Scrubbed |
| **Bearer Tokens** | `Bearer\s+[A-Za-z0-9\-\._~+/]+=*` | ✅ Scrubbed |
| **JSON Web Tokens (JWT)** | `ey[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+` | ✅ Scrubbed |
| **AWS Access Keys** | `\b(AKIA\|ABIA\|ACCA\|ASIA)[0-9A-Z]{16}\b` | ✅ Scrubbed |
| **API Keys (OpenAI, GitHub, GitLab, Google)** | `sk-...`, `ghp_...`, `glpat-...`, `AIza...` | ✅ Scrubbed |
| **Private Keys** | `-----BEGIN [A-Z\s]+PRIVATE KEY-----...` | ✅ Scrubbed |
| **Database Connection Strings** | `(postgres\|mysql\|mongodb\|redis)://...` | ✅ Scrubbed |
| **Basic Auth URLs** | `https?://user:password@host` | ✅ Scrubbed |
| **Payment Cards** | `\b(?:\d[ -]*?){13,16}\b` | ✅ Scrubbed |
| **Accidental Captures** | `is_accidental_capture` (`about:blank`, empty strings) | ✅ Filtered |
| **Generic Noise** | `is_generic_noise` (`new tab`, `untitled`, `google search`) | ✅ Filtered |

---

## 5. Verification & Test Suite

The privacy audit was validated with end-to-end integration tests in `src/context_engine/memory_e2e_tests.rs`:

- `test_scenario_10_context_memory_privacy_audit_guarantees`:
  - Verified credentials (passwords, bearer tokens, AWS keys) in source items are scrubbed in `ContextMemory` titles and references.
  - Verified `ContextWorkset` item snippets have secrets redacted.
  - Verified `ContextImportantMemory` detail snippets have secrets redacted.
  - Verified `TaskEvidence` item excerpts have secrets redacted.
  - Verified `ContextSnapshot` workset item references have secrets redacted.
- Memory Importance Quality Guard Suite (`src/context_engine/memory_importance.rs`):
  - 17 tests covering adversarial repeated generic noise, conflicting feedback, popular domain filtering, single-item context handling, and duplicate captures.

---

## 6. Conclusion

The Context Memory system complies with all privacy boundary invariants. Data minimization is enforced across all memory representations, reference structures prevent payload duplication, and active privacy scrubbing eliminates credentials and auth tokens from working memory.
