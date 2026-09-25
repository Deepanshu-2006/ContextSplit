# Windows Interaction Surface Architecture

This document defines the architectural guidelines and state machine for the future Windows interaction surface in ContextSplit.

## Core Design Principle
Unlike macOS, Windows laptops and desktops do not have a hardware "notch" to anchor a persistent drag-and-drop surface. However, the conceptual interaction flow must remain identical. 

Whether the final Windows visual implementation is a top-center floating pill, a screen edge interaction zone, or a compact always-on-top widget, the Windows surface MUST consume the **exact same Context Interaction APIs** as the macOS Notch and strictly avoid duplicating Context Engine logic.

---

## Unified State Machine

The Windows surface will support the identical state machine used by the macOS Notch. This guarantees that all platform UIs are purely reactive presentation layers driven by the same backend states.

### 1. `IDLE`
- **Description:** The default state. The interaction zone is hidden, highly transparent, or resting minimally at the screen edge/center.
- **Visuals:** Minimal visual footprint.

### 2. `HOVER`
- **Description:** The user's cursor approaches the designated interaction zone without a drag payload.
- **Visuals:** The surface subtly highlights or peeks to indicate interactivity.

### 3. `EXPANDED`
- **Description:** The surface expands to reveal quick actions or the current context status.
- **Triggers:** The user dwells in `HOVER` or clicks the interaction area.

### 4. `DRAG_ENTER`
- **Description:** A dragged item (file, text, URL) crosses into the interaction zone's boundary.
- **API Call:** Emits `drag.enter` event to the Context Engine.
- **Visuals:** The surface morphs/expands into a clear drop target.

### 5. `DRAG_ACTIVE`
- **Description:** The dragged item is held over the interaction zone.
- **API Call:** Continuously emits `drag.over`.
- **Visuals:** Spring physics or tension effects react to cursor movement.

### 6. `ACTION_READY`
- **Description:** The Context Engine resolves the candidate context or suggested intent via the Interaction API.
- **Visuals:** The UI updates dynamically (e.g., changes text to "Add to Active Context") to preview the resulting action.

### 7. `PROCESSING`
- **Description:** The user drops the payload inside the interaction boundary.
- **API Call:** Invokes `captureItem(payload)` through the Interaction API.
- **Visuals:** Displays a loading, ingestion, or processing animation.

### 8. `SUCCESS`
- **Description:** The Context Engine confirms the capture and routes the memory.
- **API Call:** Reacts to the `capture.success` signal.
- **Visuals:** Positive visual confirmation (e.g., a green checkmark or satisfying morph).

### 9. `ERROR`
- **Description:** The capture is rejected, unsupported, or fails semantic validation.
- **API Call:** Reacts to the `capture.rejected` signal.
- **Visuals:** Negative visual feedback (e.g., a red shake or "X" mark).

### 10. `COLLAPSED`
- **Description:** The surface retreats to its minimal form.
- **Triggers:** Follows `SUCCESS`, `ERROR`, or if the user cancels the interaction (`drag.leave`).
- **Visuals:** Rapidly shrinks or slides away before resetting to `IDLE`.

---

## Strategic UI Abstraction

To support this unified state machine across wildly different OS environments:

1. **Frontend Isolation:** The React/frontend layer for the "Drop Target" will be strictly separated from the Tauri OS-windowing layer.
2. **Window Transparency:** On Windows, Tauri will utilize transparent, borderless, always-on-top window configurations to simulate a "floating" or "edge" zone.
3. **API Parity:** Both the macOS and Windows frontend implementations will import and consume `src/services/contextInteractionService.ts` to manage these states. No platform-specific business logic is permitted in the UI layer.
