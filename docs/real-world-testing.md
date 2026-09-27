# ContextSplit Real-World Testing Mode

**Day 29 — Real-World Mac Testing**  
**Operating Environment:** macOS Desktop & Multi-Workspace Activity  
**Status:** Real-World Test Mode Controller Prepared & Verified

---

## Overview

Real-World Testing Mode allows controlled observation of the existing Context Engine during actual daily desktop usage (e.g. Xcode, VSCode, Chrome, Terminal, Slack, notes). 

It records development-safe, privacy-preserving diagnostics to analyze context detection accuracy, transition latency, working set stability, and resume readiness without modifying any underlying contextual decision logic.

---

## 1. What Is Recorded

When Real-World Test Mode is active, the engine records structured diagnostic envelopes for:

- **Detected Context:** Selected context ID and name, confidence score, and detection latency.
- **Candidate Contexts & Ranks:** Full list of candidate workspaces evaluated with respective heuristic scores and ranking.
- **Rejected Candidates & Reasons:** Candidate contexts eliminated by ambiguity thresholds or quality guards with explanatory reasons.
- **Evidence Used:** Categories (file path, active window title, app name, git branch, URL domain), normalized strength ratings, and correlation groups.
- **Context Transitions:** Source context, destination context, transition type (`context_switch`, `context_return`, `context_start`, `context_end`), and trigger source.
- **Task Changes:** Task candidates detected, confidence levels, status transitions (`active`, `completed`, `dismissed`), and trigger events.
- **Memory Changes:** Important memory promotions, decay steps, and working set memory tier changes.
- **Working Set Changes:** Top working set item titles, item counts, and derivation latency.
- **Lifecycle Changes:** Dynamic state shifts (`Active -> Paused -> Dormant -> Active`) and trigger explanations.
- **User Feedback:** Manual corrections (`MoveItem`), explicit confirmations (`ConfirmContext`), and suggestion rejections (`RejectSuggestion`).
- **Cold Reconstruction:** Time-to-reconstruct latency, item counts per section, and cold-start cache misses.
- **Resume Readiness:** Dynamic readiness score, `can_resume` boolean, and discovered blocking factors.
- **Processing Latency:** Millisecond/microsecond timings for each subsystem operation.

---

## 2. What Is NOT Recorded (Privacy & Safety Protections)

ContextSplit enforces strict local-first privacy guards. The following are **never** recorded:

- ❌ **No Passwords or Credentials:** Passwords in command lines, config snippets, or form fields are automatically scrubbed (`password=[REDACTED]`).
- ❌ **No Tokens or API Keys:** GitHub tokens (`ghp_...`), OpenAI keys (`sk-...`), and Bearer auth headers are stripped (`[API_KEY_REDACTED]`).
- ❌ **No Private File Contents:** Raw document bodies, source code blobs, and clipboard text payloads are not stored in diagnostic traces. Only sanitized metadata (e.g., file paths, titles) is recorded.
- ❌ **No User Identity in Paths:** Local macOS usernames (`/Users/<username>/...`) are anonymized to `/Users/developer/...`.
- ❌ **No Remote Telemetry Transmission:** All diagnostics remain in-memory or in local test files; zero network requests are dispatched to external servers.

---

## 3. How to Start a Test Session

### In Rust Backend / CLI / Test Runner:
```rust
use crate::context_engine::real_world_test_mode::get_real_world_test_controller;

let controller = get_real_world_test_controller();
let session = controller.start_session(
    "Morning Dev Session",
    Some("Observing ContextSplit & PayX development on macOS"),
)?;

println!("Test Session Started: {} ({})", session.session_name, session.session_id);
```

### In TypeScript / Frontend Interaction Layer:
```typescript
import { contextInteractionService } from './services/contextInteractionService';

// Activate Real-World Test Mode session
await contextInteractionService.startTestSession("Morning Dev Session", "macOS Live Observation");
```

---

## 4. How to End a Test Session

### In Rust Backend / CLI / Test Runner:
```rust
let summary = controller.stop_session()?;

println!("Test Session Completed!");
println!("Total Events: {}", summary.total_events);
println!("Avg Detection Latency: {:.2}ms", summary.average_detection_latency_ms);
println!("Avg Reconstruction Latency: {:.2}ms", summary.average_reconstruction_latency_ms);
```

### In TypeScript / Frontend Interaction Layer:
```typescript
const summary = await contextInteractionService.stopTestSession();
console.log("Test Session Summary:", summary);
```

---

## 5. How Diagnostics Are Inspected

1. **In-Memory Querying:**
   ```rust
   let recent_events = controller.get_recent_events(50);
   let session_events = controller.get_session_events(&session_id);
   ```
2. **Automated Markdown Report Generation:**
   ```rust
   if let Some(report) = controller.generate_session_report(&session_id) {
       println!("{}", report);
   }
   ```
3. **Session Diagnostics Breakdown:**
   Each recorded envelope contains timestamps, event index, and typed payload (`ContextDetection`, `ContextTransition`, `TaskChange`, `WorksetChange`, `Feedback`, etc.).

---

## 6. How Test Data Is Separated from Normal Data

- **Explicit Flagging & Labeling:** All test session envelopes are marked with `is_test_mode: true` and indexed by a unique `session_id`.
- **Dedicated Ring Buffers:** Test events reside in a dedicated bounded buffer (`MAX_REAL_WORLD_EVENTS = 5000`) rather than polluting production user settings.
- **Zero Overhead in Production:** When no test session is active (`is_active() == false`), diagnostic calls return immediately with no memory allocation or I/O overhead.
- **Deterministic Purge Capability:** Calling `controller.clear_all_test_sessions()` completely resets all test telemetry without affecting persistent SQLite workspace items.

---
