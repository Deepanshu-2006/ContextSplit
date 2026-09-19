# Context Accuracy Evaluation Dataset

Deterministic synthetic dataset for evaluating Context Engine accuracy, false-context defense, and context transition detection.

## Invariant
**DO NOT USE THIS DATASET TO TRAIN MODELS.**
It is strictly used as an evaluation benchmark and regression test suite.

## Defined Contexts
1. **PayX (`ws_payx`)**: Payment architecture, checkout frontend, payment API docs, repository.
2. **ContextSplit (`ws_contextsplit`)**: Context engine, desktop app UI, launcher, transitions, Tauri codebase.
3. **SwiftSend (`ws_swiftsend`)**: Upload component, Cloudinary, Firebase storage, repository.
4. **University OS (`ws_university_os`)**: OS scheduling assignment, C++ code, lecture notes, canvas LMS.
5. **Tokyo Trip (`ws_tokyo_trip`)**: Flights (ANA), hotel bookings (Booking.com), itineraries, travel maps.

## Covered Categories
- **Clean cases**: High certainty activities with direct file path/repo anchors.
- **Noisy cases**: Social media (Instagram), video entertainment (YouTube), shopping (Amazon), music (Spotify), and unrelated repos (Linux kernel).
- **Ambiguous cases**: Generic GitHub dashboards, empty VS Code tabs, blank terminals competing across candidate contexts.
- **Context switch cases**: Corroborated sequences transitioning focus from PayX to ContextSplit.
- **Misleading application cases**: Coding IDEs (VS Code) used for academic assignments, or browsers (Chrome) used for personal travel planning.
- **Duplicate cases**: Rapid focus bounce / deduplication stability.
- **Delayed evidence cases**: Ambiguous root domains resolved by subsequent repository navigation.
