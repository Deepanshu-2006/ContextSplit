# Context State Architecture Audit

## 1. Existing State-Related Data
The current architecture provides a robust foundation for determining "Where did the user leave this work?" through several derived structures:
- **`ContextWorkset`**: Represents the most relevant, stable set of items for a context. It tracks the `workset_state` (Active, RecentlyActive, DetourReturn, Dormant), the active `task_title`, `recent_activity` summaries, and item relevance tiers (Primary, Supporting, Reference, Recent).
- **`ContextOverview` & `ContextTimeline`**: Provide chronological session tracking, including session starts/ends, items captured per session, user actions, and contextual events.
- **`TaskCandidate` & `TaskSnapshot`**: Track inferred tasks and their lifecycle (`TaskCandidateStatus` like Active, Inactive, Uncertain, Dismissed). 
- **Transitions (`ContextTransition`, `DetectedTaskTransition`)**: Emit events for state changes like `ContextSwitch`, `ContextReturn`, `ContextEnd`, `TaskStart`, `TaskEnd`, `TaskSwitch`.
- **Item Metadata**: `Item` and `ContextSignal` schemas allow embedding JSON metadata, tracking `item_type` (url, file, image, text), and timestamps.

## 2. Reusable Services
- **`ContextWorksetBuilder`**: Synthesizes relationships, memory, and signals into a stable working set, automatically applying hysteresis and filtering noise.
- **`TaskTransitionDetector`**: Processes candidate tasks sequentially to detect boundaries and emit stable task transitions (with anti-jitter logic).
- **`ContextActivitySegmenter`**: Evaluates temporal gaps to group activities.
- **`ContextWorksetCache`**: Provides thread-safe, fast retrieval of the latest contextual working state.

## 3. Missing Information
- **Granular Focus State**: While we know the "Primary" items, we do not inherently track exact application state (e.g., cursor position, scroll depth, precise UI focus within a web page) unless explicitly captured by the client and sent in item metadata.
- **Transient Drafts**: Unsaved text or transient clipboard state not committed as an `Item` is not tracked in the current relational model.
- **Window Geometries**: The exact layout of how windows were arranged on the screen is not standardly retained in the core logic.

## 4. Potential Ambiguity
- **Ambiguous Transitions**: `ContextTransitionType::AmbiguousTransition` and `TaskTransitionType::AmbiguousTaskChange` mean the "departure" point might be blurry (e.g., user drifted into unrelated browsing before explicitly leaving the context).
- **Dormant Worksets**: If a context has been `Dormant` for weeks, the "last known state" might be heavily decayed, making it unclear if the user finished the work or abandoned it.
- **Accidental Captures**: The last recorded item might be a generic "New Tab" or accidental capture just before sleep, which could mislead the reconstruction if not filtered.

## 5. Recommended Architecture
To reconstruct the **Last Known State** (Working State Reconstruction), we should introduce a derived layer that consumes existing data without duplicating records:
- **`WorkingStateService`**: A service that queries the `ContextWorkset` and the latest `ContextTimeline` session to project a `WorkingStateSnapshot`.
- **Resume Point Identification**: Combine the `last_active_at` timestamp from `ContextWorkset` with the latest `TaskSnapshot` from the `TaskTransitionDetector` to summarize the departure point.
- **Snapshot Caching**: Hook into the `ContextTransitionDetector` to capture and cache a `WorkingStateSnapshot` explicitly when a `ContextEnd` or `ContextSwitch` is emitted.

## 6. State Reconstruction Rules
1. **Identify the Boundary**: Use `ContextTransition` logs to find the exact timestamp of the last `ContextEnd` or `ContextSwitch`.
2. **Determine Last Task**: Check the `TaskCandidate` table for the task that was `Active` precisely at that boundary timestamp.
3. **Filter Materials**: Use the `ContextWorksetBuilder` as of the departure timestamp. Retain items classified as `Primary` or `Supporting`. 
4. **Suppress Noise**: Apply `is_generic_noise` and `is_accidental_capture` from `workset.rs` to ensure the "last item" wasn't a stray click on the desktop or an empty browser tab.
5. **Generate Summary**: Combine the active task title and the most recent `TimelineUserAction` into a single natural language string (e.g., "Drafting Case study writing, last viewed 'Figma Mockup'").

## 7. Privacy Considerations
- The "Last Known State" inherently centralizes highly contextual user data (last viewed titles, active tasks, sensitive URLs).
- This reconstructed state must remain strictly local and securely managed, mirroring the protections on `ContextMemory`.
- Care must be taken not to include `Dismissed` items or `Dismissed` contexts in any state reconstruction, honoring user suppression signals.
- Summaries generated from the state must avoid logging to external telemetries.

## 8. Test Requirements
- **Clean Departure Test**: Emulate a `ContextSwitch` and verify the reconstructed state captures the exact `TaskSnapshot` and `Primary` items present at that moment.
- **Drift Departure Test**: Simulate ambiguous drift (noisy generic captures before a gap); verify the reconstructor ignores the noise and accurately surfaces the last substantive task.
- **Immediate Return Test**: Ensure that a `ContextReturn` transition correctly aligns with the previously cached "Last Known State".
- **Dormant Context Test**: Verify behavior when reconstructing a state from >14 days ago, ensuring temporal decay appropriately reflects the age of the state.
