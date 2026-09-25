# macOS Notch Interaction State Machine

This document outlines the state machine for the future macOS Notch interaction surface in ContextSplit. 

## Core Design Principle
The macOS Notch UI is a **dumb presentation layer**. It does not possess any native contextual intelligence, nor does it run models or database queries. Its only responsibilities are to render visual states based on the physical cursor/drag interactions and to consume the `Context Interaction API` to dispatch events to the backend Context Engine.

---

## State Definitions

### 1. `IDLE`
- **Description:** The default state. The notch is hidden or resting natively at the top of the screen.
- **Triggers:** System starts, or a previous interaction gracefully completes or cancels.
- **Visuals:** Minimal to no footprint.

### 2. `HOVER`
- **Description:** The user's cursor approaches the notch area without an active drag payload.
- **Triggers:** Cursor proximity.
- **Visuals:** A subtle glow or hint indicating interactivity.

### 3. `EXPANDED`
- **Description:** The notch expands to reveal quick actions or context statuses.
- **Triggers:** The user dwells in `HOVER` or explicitly clicks the notch area.
- **Visuals:** Expanded menu dropping down smoothly from the hardware notch.

### 4. `DRAG_ENTER`
- **Description:** A dragged item crosses the notch interaction boundary.
- **Triggers:** OS physical drag entering the designated rect.
- **API Call:** Emits `drag.enter` event to the Context Engine.
- **Visuals:** The notch immediately morphs/expands to indicate it is a valid drop target.

### 5. `DRAG_ACTIVE`
- **Description:** The user holds a dragged item over the expanded notch while the system prepares.
- **Triggers:** Continuation of `DRAG_ENTER`.
- **API Call:** `drag.over` is continuously or periodically emitted.
- **Visuals:** Tension or spring physics react to the cursor's movement.

### 6. `ACTION_READY`
- **Description:** The Context Engine has resolved the context candidate or intent.
- **Triggers:** The backend emits a contextual insight (e.g., "Add to TripWise") via the Interaction API.
- **Visuals:** The notch UI dynamically updates to show the resolved context or specific action (e.g., changing colors or displaying the context name).

### 7. `PROCESSING`
- **Description:** The user drops the item. The notch is actively handling the payload handoff.
- **Triggers:** User releases the mouse button (`drag.drop`).
- **API Call:** `captureItem(payload)` is invoked via the Interaction API.
- **Visuals:** A loading or ingestion animation.

### 8. `SUCCESS`
- **Description:** The Context Engine successfully processed and assigned the capture.
- **Triggers:** The backend emits a `capture.success` signal.
- **Visuals:** A positive confirmation, such as a green checkmark or a satisfying "snap" animation.

### 9. `ERROR`
- **Description:** The capture was rejected, unsupported, or failed validation.
- **Triggers:** The backend emits a `capture.rejected` signal.
- **Visuals:** A distinct negative visual, such as a red shake or "X" mark.

### 10. `COLLAPSED`
- **Description:** The final phase of interaction, quickly retreating before resetting.
- **Triggers:** Follows `SUCCESS`, `ERROR`, or if the user cancels the interaction (drag leaves boundary).
- **Visuals:** Rapidly shrinks back into the hardware notch before entering the `IDLE` state.

---

## State Transition Flow Example

1. `IDLE`
2. User drags a file toward the notch.
3. `DRAG_ENTER` (notch detects the file).
4. `DRAG_ACTIVE` (user dwells).
5. ContextSplit Interaction API resolves candidate behind the scenes.
6. `ACTION_READY` (notch updates to say "Add to Active Context").
7. User drops the file.
8. `PROCESSING` (payload sent to engine).
9. Context Engine confirms storage and routing.
10. `SUCCESS` (visual confirmation).
11. `COLLAPSED` (notch shrinks).
12. `IDLE` (reset for next interaction).
