# macOS Notch Capture Interaction Prototype

## 1. Overview & Core Concept

The **macOS Notch Capture** interaction prototype transforms the hardware notch on modern MacBook Pro and MacBook Air displays into a native, physical capture portal for **ContextSplit**.

Unlike a conventional floating window, notification popup, or web card, the MacBook's physical notch serves as the anchor and origin of the interaction:

```
              █████████
              █ PHYSICAL █
              █  NOTCH   █
              █████████
                   ↓
              Notch expands
                   ↓
          ╭────────────────╮
          │ Release to     │
          │ Capture        │
          │    TripWise    │
          ╰────────────────╯
```

When idle, the notch remains completely invisible (`height: 0px`, `opacity: 0`) with zero visual footprint or obstruction to the Mac desktop. As the user begins dragging supported content (text snippets, URLs, images, or files) and approaches the top center of the screen:
1. The physical notch wakes up subtly.
2. The dragged item experiences magnetic attraction toward the notch.
3. The surface smoothly expands into an Apple Dynamic Island / NotchNook-inspired interactive pill.
4. When inside the drop target, it shows the active workspace context ("Release to Capture" / "TripWise").
5. Upon release, the dragged thumbnail is magnetically absorbed into the center of the notch.
6. A refined confirmation appears ("✓ Added to TripWise").
7. The surface smoothly contracts and recedes back into the physical notch.

---

## 2. System Architecture & Modular Design

The interaction layer is designed with clean modular abstractions:

```
┌─────────────────────────────────────────────────────────┐
│                      macOS AppKit                       │
│  (NSStatusWindowLevel 25, Transparent, Click-Through)   │
└───────────────────────────┬─────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│            NotchInteractionController (Rust)            │
│  • NotchGeometry (Retina scale, display bounds, center) │
│  • Proximity calculation & Euclidean distance tracking  │
│  • TargetState ↔ NotchInteractionState bidirectional map│
└───────────────────────────┬─────────────────────────────┘
                            │ (Events & Ingestion)
                            ▼
┌─────────────────────────────────────────────────────────┐
│             NotchInteraction (Frontend)                 │
│  ├── NotchStateMachine (8 canonical interaction states) │
│  ├── Dynamic Island Surface (Hardware black + ears)     │
│  ├── Magnetic Dragged Thumbnail (Velocity tilt & pull)  │
│  └── NotchAnimator (Apple spring curves & specs)        │
└───────────────────────────┬─────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────┐
│                  CaptureService (Core)                  │
│  • Canonical CaptureEvent normalization                 │
│  • Pipeline dispatch & Context Engine derivation        │
└─────────────────────────────────────────────────────────┘
```

### Module Responsibilities:
1. **`NotchInteraction`** (`src/components/notch/NotchInteraction.tsx`): Dedicated component rendering the physical notch surface anchored directly to `top: 0`, coordinating Tauri events, workspace context resolution, proximity tracking, magnetic drag pull, thumbnail absorption, and Option+D developer HUD.
2. **`NotchStateMachine`** (`src/components/notch/NotchStateMachine.ts`): Implements the 8 responsive states (`IDLE`, `DRAG_STARTED`, `DRAG_APPROACHING`, `DROP_ZONE_ACTIVE`, `CAPTURING`, `CAPTURE_SUCCESS`, `CAPTURE_CANCELLED`, `RETURN_TO_IDLE`).
3. **`NotchAnimator`** (`src/components/notch/NotchAnimator.ts`): Contains Apple spring easing curves (`cubic-bezier(0.16, 1, 0.3, 1)`), spatial dimensions per state, non-linear magnetic attraction formulas, and velocity tilt calculations.

---

## 3. Physical Notch Silhouette & Concave Ear Fillets

To ensure the surface visually emerges directly from the physical notch bezel:
- **Anchoring at Top Edge (`top: 0`):** Pinned flush to the top of the screen (`sf.origin.y + sf.size.height - win_h` in Cocoa, `top: 0` in webview).
- **Hardware Pitch Black:** Exact `#000000` surface matching the MacBook camera housing and display bezel.
- **Concave Ear Fillets:** Left and right SVG arcs at `top: 0` blend the vertical sides of the expanding notch into the horizontal top bezel:
  - Left ear: `M 0 0 C 5.52 0 10 4.48 10 10 L 10 0 Z`
  - Right ear: `M 0 10 C 0 4.48 4.48 0 10 0 L 0 0 Z`
- **Ambient Occlusion Rim Catch:** Soft, subtle ambient shadow (`rgba(0, 0, 0, 0.75)`) with a delicate inset bottom rim highlight (`inset 0 -0.5px 0.5px rgba(255, 255, 255, 0.12)`). No neon glows or gaming RGB lighting.

---

## 4. Interaction States & Spring Physics Lifecycle

The interaction progresses through 8 responsive states:

```
  IDLE (0px height, invisible, zero obstruction)
   │
   ▼ (drag detected anywhere on display)
  DRAG_STARTED (178×32px hardware footprint wakes up)
   │
   ▼ (cursor approaches within radius <260px)
  DRAG_APPROACHING (260×64px: expanding downward, "Capture to TripWise")
   │
   ▼ (cursor enters drop target zone)
  DROP_ZONE_ACTIVE (340×122px Dynamic Island: Header, settled thumbnail inside, "● DROP")
   │
   ▼ (mouse release)
  CAPTURING (320×110px: thumbnail absorbed into notch mouth + brief notch squish)
   │
   ▼ (capture processed)
  CAPTURE_SUCCESS (250×48px confirmation pill: "✓ Captured to TripWise")
   │
   ▼ (500ms hold)
  RETURN_TO_IDLE / IDLE (smoothly retracts back into physical notch)
```

---

## 5. Dragged Item Magnetic Attraction & Absorption

1. **Velocity Tilt:** As the user moves the cursor horizontally, the drag preview tilts slightly (`rotate(±8deg)`) based on cursor velocity, providing natural physical inertia.
2. **Magnetic Pull:** As the cursor nears the notch within 260pt, the preview experiences non-linear ease-in pull toward the notch mouth.
3. **Settled Thumbnail Inside Container:** When the cursor reaches within 80pt or enters the drop zone, the thumbnail settles directly *inside* the expanded 122pt-tall notch container, positioned beneath the header and above the drop badge.
4. **Absorption Sequence on Drop:**
   - Detect drop release.
   - Thumbnail inside notch glides upward directly into the notch camera center (`translateY(-42px)`, `scale(0.12)`, `opacity(0)`).
   - Notch morphs into compact capturing compression (`scale(0.98)`).
   - Confirmation displays "Captured to TripWise" with animated checkmark.
   - Contracts smoothly back into the physical notch.

---

## 6. Native Window Attributes & Display Geometry

The overlay window (`macos-notch-target`) is configured with native AppKit attributes:
- **Level 25 (`NSStatusWindowLevel`):** Floats above standard application windows and the macOS menu bar.
- **CanJoinAllSpaces | FullScreenAuxiliary | Stationary | IgnoresCycle:** Stays visible across virtual Desktops and Mission Control without stealing focus or appearing in the dock.
- **Dynamic Retina Scaling:** Multi-monitor and Liquid Retina display resolutions (`NSScreen backingScaleFactor`) are resolved dynamically without hardcoded coordinates.
- **Click-Through When Idle:** The window ignores clicks when idle and has `pointer-events: none` on transparent areas.

---

## 7. Developer Diagnostics HUD (Option+D)

During development (`import.meta.env.DEV`), pressing **Option+D** (or `Alt+D`) toggles a compact telemetry HUD displaying:
- Screen geometry and Retina scale factor (e.g. 1512×982 @ 2x).
- Live cursor position `(X, Y)`.
- Euclidean distance to the notch center in logical points.
- Normalized proximity ratio (0.0 at center, 1.0 at activation radius).
- Drop target containment flag.
- Current active workspace context name.
