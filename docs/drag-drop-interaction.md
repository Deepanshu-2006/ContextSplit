# Cross-Platform Drag & Drop Interaction Contract

## Vision
ContextSplit relies on seamless file, text, and link ingestion via Drag & Drop interaction surfaces. To ensure long-term stability and consistent behavior across operating systems (macOS Notch, Windows Edge, future Linux targets), the drag & drop layer must adhere to a strict platform-independent contract. 

Platform-specific OS code (e.g., Swift/Objective-C on macOS or C#/C++ on Windows) is responsible for detecting the physical drag and forwarding abstract events to the frontend and Context Engine. The backend Context Engine is strictly decoupled from any OS-specific drag APIs.

---

## 1. Lifecycle Events

Regardless of the OS surface, a drag-and-drop interaction MUST emit the following normalized lifecycle events to the Context Engine UI API:

- **`drag.enter`**: Fired when a dragged item physically intersects the designated interaction boundary.
- **`drag.over`**: Fired continuously or strategically as the item is moved within the interaction boundary (useful for hover animations or spring physics).
- **`drag.leave`**: Fired when the dragged item exits the interaction boundary or the user aborts the physical drag using the OS (e.g., hitting `Esc`).
- **`drag.drop`**: Fired when the user successfully releases the item inside the designated interaction boundary.

---

## 2. Interaction Payload

When a `drag.drop` event successfully concludes, the surface layer extracts the OS-level data and transforms it into a standard `CapturePayload`. 

```typescript
interface CapturePayload {
  sourceSurface: 'mac_notch' | 'windows_edge' | 'app';
  timestamp: string;
  items: Array<{
    type: 'file' | 'url' | 'text' | 'image';
    content: string; // The raw content or file path
    metadata?: {
      mimeType?: string;
      filename?: string;
      originatingApp?: string; 
    }
  }>;
}
```

---

## 3. Validation

The platform surface performs preliminary validation. The Context Engine performs secondary semantic validation:
- **Platform Layer:** Rejects unrecognizable formats, empty strings, or unsupported OS promises before sending to the engine.
- **Engine Layer:** Accepts the standard payload. If the payload is determined to be non-actionable (e.g., a file path that doesn't exist, an unreadable format), the engine immediately triggers a **Rejection** phase.

---

## 4. Context Resolution

Upon receiving a valid `CapturePayload`, the Context Engine executes the Context Discovery pipeline:
1. **Item Analysis:** Extracts text/features from the captured items.
2. **Context Matching:** Determines if the item is relevant to the **Active Context**, a **Candidate Context**, or an entirely unrelated context.
3. **Suggested Action:** Determines if the item should be automatically assigned (e.g., high confidence match) or if a contextual insight should be pushed back to the UI (e.g., "Add to TripWise?").

---

## 5. Success Phase

If the Context Engine successfully ingests the item and routes it into the memory pipeline, it broadcasts a `capture.success` event. 
- The platform UI MUST display a brief, positive visual acknowledgment (e.g., a green checkmark or spring animation).
- Following success, the UI gracefully resets to an **IDLE** state.

---

## 6. Rejection Phase

If the Context Engine determines the item is invalid, unsupported, or explicitly rejected by a business rule, it broadcasts a `capture.rejected` event.
- The platform UI MUST display a distinct visual error (e.g., a red shake or "X" mark).
- The item is discarded.
- Following rejection, the UI gracefully resets to an **IDLE** state.

---

## 7. Cancellation

A drag interaction can be cancelled at any point before `drag.drop` (e.g., the user drags the item away).
- The `drag.leave` event is fired.
- The platform UI immediately resets to an **IDLE** state without triggering validation or resolution pipelines. 
- The Context Engine does not receive any partial capture data.
