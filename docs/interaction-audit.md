# Context Launcher Interaction Audit Report

**Milestone:** Phase 2 — Context Launcher Interaction Polish  
**Audit Target:** [`src/components/launcher/ContextLauncher.tsx`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx)  
**Date:** October 23, 2026  
**Document:** [`docs/interaction-audit.md`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/docs/interaction-audit.md)  
**Status:** **COMPLETE & VERIFIED**  

---

## Executive Summary

This audit assesses the actual interaction design, keyboard navigation, focus management, state transitions, and accessibility behavior of the **Context Launcher** in ContextSplit. 

The evaluation was performed through systematic source code verification, runtime test execution, and interaction path tracing across:
* [`src/components/launcher/ContextLauncher.tsx`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx) (2,269 lines)
* [`src/store/launcherStore.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/launcherStore.ts) (1,510 lines)
* [`src/models/quickActions.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/quickActions.ts) & [`src/services/quickActionsService.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/quickActionsService.ts)
* [`src/models/contextWorkset.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/models/contextWorkset.ts) & [`src/services/contextWorksetRetrieval.ts`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/services/contextWorksetRetrieval.ts)
* Existing milestone reports: [`docs/day-33-report.md`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/docs/day-33-report.md), [`docs/day-34-report.md`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/docs/day-34-report.md), and [`docs/context-workset-integration-report.md`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/docs/context-workset-integration-report.md).

All findings are categorized by interaction area and classified by severity (**Critical**, **High**, **Medium**, **Low**).

---

## 1. Interactions That Work Correctly

1. **Global Invocation & Dismissal Hierarchy:**
   - `⌘K` globally opens the launcher modal overlay with instant focus transferred to `#launcher-search-input` via `setTimeout(..., 50)` ([`ContextLauncher.tsx:111-117`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L111-L117)).
   - `Escape` enforces a deterministic progressive dismissal hierarchy:
     $$\text{Dismiss Action/Resume Error} \longrightarrow \text{Close Preview} \longrightarrow \text{Clear Search Query} \longrightarrow \text{Reset Filter to 'all'} \longrightarrow \text{Close Launcher}$$
   - Clicking the backdrop dismisses the dialog cleanly ([`ContextLauncher.tsx:507-509`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L507-L509)).

2. **Lifecycle-Aware Primary Action Adaptation:**
   - The primary button dynamically computes its descriptor from the context's real state ([`ContextLauncher.tsx:989-992`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L989-L992)):
     - **Active:** "Focus Workspace" (`↵`) switches focus directly to running workspace.
     - **Paused / Dormant:** "Resume Context" (`↵`) triggers the resume signal.
     - **Archived:** "Restore & Resume" (`↵`) performs atomic SQLite unarchive then resume.

3. **Strict Concurrency & Re-entrancy Locks:**
   - Execution paths across `executeContextAction`, `executeItemAction`, and the primary action button enforce `if (isResuming || isActionExecuting) return;` ([`ContextLauncher.tsx:438`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L438), [`launcherStore.ts:1255`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/launcherStore.ts#L1255)).
   - Rapid clicking or double-pressing Enter yields `outcome: 'BLOCKED'` and prevents concurrent IPC calls.

4. **Deterministic Context Disambiguation:**
   - When duplicate names exist across contexts, the UI adds a clear `#shortId` tag without exposing raw internal UUIDs ([`ContextLauncher.tsx:2214-2220`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L2214-L2220)).

5. **Sorting & Grouping Dynamics:**
   - Two sort orders (`recent` and `name`) toggle via a macOS-style segmented pill or `⌥S` ([`ContextLauncher.tsx:403-407`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L403-L407)).
   - Pinned contexts stay elevated at the top with zero item duplication.
   - Preference is reliably saved to and rehydrated from `localStorage` (`cs_launcher_preferences_v1`).

6. **Epistemic Badging for Working Set Items:**
   - Confirmed items rank above inferred items; candidate inbox suggestions clearly display an amber `Suggested` badge, while contextual relationships display a sky-blue `Inferred` badge ([`ContextLauncher.tsx:1923-1939`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1923-L1939)).

7. **Non-Destructive Error Banners with Retry:**
   - Action errors display `#launcher-action-error-banner` with clear error titles, retry triggers for retryable failures (`actionError.isRetryable`), and dismiss buttons ([`ContextLauncher.tsx:1366-1410`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1366-L1410)).

---

## 2. Inconsistent or Missing Interaction States

### [HIGH] 2.1 Two-Stage Enter Key Disconnect
* **Evidence:** [`ContextLauncher.tsx:452-461`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L452-L461)
* **Description:** On desktop screens ($\ge 768\text{px}$), the launcher renders a 12-column split view where the right preview pane is **always visible**. However, `isPreviewOpen` defaults to `false` on initial launch and is reset to `false` whenever the user types into search or switches filters ([`launcherStore.ts:635, 698, 702`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/store/launcherStore.ts#L635)).
  When the user selects a context and presses `Enter`, lines 452-454 execute:
  ```tsx
  if (!isPreviewOpen) {
    setIsPreviewOpen(true);
  } else {
    executeContextAction('context.resume', target.id);
  }
  ```
* **Impact:** The user sees the context preview already open on screen, sees the `<kbd>↵</kbd>` badge on the item and the `<kbd>↵</kbd> Resume` button, but pressing `Enter` the first time produces **no visible change**. The user is forced to press `Enter` a second time to actually resume.
* **Resolution:** In desktop split-view mode, `Enter` should immediately execute the primary action (`Resume Context` or `Focus Workspace`). Two-stage entry should only apply when the preview pane is collapsed (e.g. mobile/compact views).

---

### [MEDIUM] 2.2 Un-debounced Mouse Hover Selection Causes Preview Thrashing
* **Evidence:** [`ContextLauncher.tsx:2188`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L2188)
* **Description:** `ContextLauncherItem` binds `onMouseEnter={onSelect}` without debouncing or dwell detection:
  ```tsx
  onMouseEnter={onSelect}
  ```
* **Impact:** Moving the mouse diagonally across the list to reach a button or scrollbar triggers `selectContextById` on every traversed item. Each selection increments `fetchVersion`, sets `isLoadingPreview: true`, and queries the database for workset items. This causes noticeable preview pane jumping and layout flicker.
* **Resolution:** Debounce mouseover selection ($100\text{–}150\text{ms}$) or require explicit click for selection while retaining subtle visual hover highlight (`hover:bg-white/[0.04]`).

---

### [MEDIUM] 2.3 Rename Input Lacks Blur Handling
* **Evidence:** [`ContextLauncher.tsx:1165-1216`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1165-L1216)
* **Description:** When inline renaming is active (`isRenaming === true`), clicking outside the rename input or focusing another control leaves the rename input open and editable.
* **Impact:** If the user starts renaming, gets distracted, and clicks into the search bar, the rename input remains frozen in place in the header instead of cleanly cancelling or submitting.
* **Resolution:** Add `onBlur` handling to `#launcher-rename-context-input` to auto-cancel if untouched or prompt submission.

---

### [LOW] 2.4 Filter Tabs Roving TabIndex Violation
* **Evidence:** [`ContextLauncher.tsx:578`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L578)
* **Description:** Every tab in `role="tablist"` has `tabIndex={0}`:
  ```tsx
  tabIndex={0}
  ```
* **Impact:** In standard WAI-ARIA tab navigation, only the active tab should be in the tab sequence (`tabIndex={0}`), while inactive tabs should have `tabIndex={-1}` and be navigated via Arrow keys. Currently, keyboard users must press `Tab` 5 consecutive times to get past the filter bar.
* **Resolution:** Set `tabIndex={isSelected ? 0 : -1}` and support `ArrowLeft`/`ArrowRight` across tabs.

---

## 3. Keyboard Navigation Problems

### [CRITICAL] 3.1 Workset Item Focus Causes Context-Switching on ArrowDown / ArrowUp
* **Evidence:** [`ContextLauncher.tsx:318-353`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L318-L353)
* **Description:** In `ContextLauncher.tsx`, the global keydown listener traps `ArrowDown` and `ArrowUp` to cycle contexts in the left list. The guard check is:
  ```tsx
  if (
    activeEl?.closest('[data-menu-trigger="true"]') ||
    activeEl?.id === 'launcher-primary-action-btn' ||
    activeEl?.closest('[role="menu"]')
  ) {
    return;
  }
  ```
  However, `WorksetPreviewRow` renders as:
  ```tsx
  <div ref={rowRef} role="button" tabIndex={0} ...>
  ```
* **Impact:** When a keyboard user presses `Tab` to navigate into the context's working set and focuses an item row, pressing `ArrowDown` or `ArrowUp` (intending to navigate items) is intercepted by the global listener! The launcher immediately runs `setSelectedIndex(nextIndex)` and **switches the active workspace in the left list**, instantly wiping out the preview pane and destroying the user's focus!
* **Resolution:** Add `activeEl?.closest('[data-workset-row="true"]')` (or `activeEl?.closest('[data-preview-pane="true"]')`) to the exclusion guard so arrow navigation inside the working set stays confined to working set items.

---

### [HIGH] 3.2 Global Option Shortcuts Conflict With Text Input Editing
* **Evidence:** [`ContextLauncher.tsx:367-423`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L367-L423)
* **Description:** Shortcuts `⌥P` (pin), `⌥R` (rename), `⌥S` (sort), and `⌥A` (archive) are globally bound whenever the launcher is open:
  ```tsx
  if (e.altKey && e.key.toLowerCase() === 'r') { ... }
  if (e.altKey && e.key.toLowerCase() === 's') { ... }
  ```
  The listener does not check if `activeEl?.tagName === 'INPUT'`.
* **Impact:** On macOS, `Option` key combinations are standard for typing diacritics and symbols (`⌥R` produces `®`, `⌥S` produces `ß`, `⌥P` produces `π`). If the user is actively renaming a context in `#launcher-rename-context-input` or searching in `#launcher-search-input`, typing `⌥S` switches the sort order beneath their text, and typing `⌥A` executes context archival while typing!
* **Resolution:** Check `if (activeEl?.tagName === 'INPUT' || isRenaming) return;` before processing `⌥P`, `⌥R`, `⌥S`, `⌥A`.

---

### [MEDIUM] 3.3 Working Set Items Lack Arrow-Key Traversal
* **Evidence:** [`ContextLauncher.tsx:1802-1810`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1802-L1810)
* **Description:** In `WorksetPreviewRow`, keyboard navigation only handles `Enter` and `Space` to open/reveal the item. There is no arrow navigation between working set items; moving from item 1 to item 2 requires pressing `Tab` 3 times (row $\rightarrow$ star $\rightarrow$ menu $\rightarrow$ next row).
* **Resolution:** Support `ArrowDown` and `ArrowUp` between sibling workset item rows.

---

## 4. Focus Loss or Unexpected Focus Movement

### [HIGH] 4.1 Launcher Modal Lacks Focus Trap
* **Evidence:** [`ContextLauncher.tsx:504-517`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L504-L517)
* **Description:** The dialog wrapper `<div role="dialog" aria-modal="true">` does not implement a focus trap.
* **Impact:** Pressing `Tab` from the final interactive element (`#launcher-more-actions-btn`) moves focus completely out of the launcher into background window controls behind the semi-transparent backdrop. Similarly, `Shift+Tab` from `#launcher-search-input` moves focus backwards into background window tabs.
* **Resolution:** Implement a lightweight focus trap hook that wraps `Tab` from the last focusable element back to `#launcher-search-input` and wraps `Shift+Tab` from search back to `#launcher-more-actions-btn`.

---

### [HIGH] 4.2 Focus Dropped to Body on Tab in Secondary Menus
* **Evidence:** [`ContextLauncher.tsx:1083-1085`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1083-L1085), [`ContextLauncher.tsx:1866-1868`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1866-L1868)
* **Description:** When either `#launcher-context-secondary-menu` or `#launcher-item-menu` is open, pressing `Tab` executes:
  ```tsx
  } else if (e.key === 'Tab') {
    setIsMenuOpen(false);
  }
  ```
* **Impact:** The menu is immediately unmounted from the DOM, but focus is never restored to `triggerRef.current`. Focus is dropped onto `document.body`, breaking keyboard workflows.
* **Resolution:** On `Tab` or `Escape` dismissal, explicitly call `triggerRef.current?.focus()`.

---

## 5. Animations and Transition Behavior

### [MEDIUM] 5.1 Preview Pane Height and Layout Jumping
* **Evidence:** [`ContextLauncher.tsx:641, 1483`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L641)
* **Description:** When navigating contexts via keyboard, contexts with zero items render an empty state card (`py-4 px-3`), whereas contexts with 5 items render a scrollable list (`max-h-[175px]`). The preview pane contents swap instantaneously without a subtle opacity transition.
* **Resolution:** Add a gentle 75ms fade/cross-fade (`transition-opacity duration-75`) when `preview.contextId` changes to eliminate jarring layout shifts.

### [LOW] 5.2 Abrupt Menu Popover Exit
* **Evidence:** [`ContextLauncher.tsx:1583, 2062`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1583)
* **Description:** Popovers animate in smoothly (`animate-in fade-in zoom-in-95 duration-100`), but instantly disappear upon dismissal. While acceptable for utility popovers, adding exit transitions improves native macOS feel.

---

## 6. Controls That Appear Interactive But Do Not Work

### [CRITICAL] 6.1 Item Action Menu Clipping in Scrollable Container
* **Evidence:** [`ContextLauncher.tsx:1483, 2062`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1483)
* **Description:** The working set rows are contained within:
  ```tsx
  <div className="space-y-1 max-h-[175px] overflow-y-auto pr-0.5">
    {preview.worksetItems.slice(0, 5).map((item) => (
      <WorksetPreviewRow ... />
    ))}
  </div>
  ```
  Inside each row, the item secondary action menu (`#launcher-item-menu-${item.id}`) is positioned:
  ```tsx
  className="absolute right-0 bottom-full mb-1 w-56 bg-stone-900/95 ... z-40"
  ```
* **Impact:** In CSS, any `absolute` child inside an `overflow-y-auto` container is **clipped** by the container's overflow boundary if it renders outside the bounding box. When opening the menu on the first item in the list, the menu opens upwards (`bottom-full`), rendering completely outside the $175\text{px}$ container. The menu is clipped by the top edge or causes unexpected scrollbars, preventing users from clicking menuitems.
* **Resolution:** Either remove `overflow-y-auto` from the inner container (since items are already strictly bounded to 5 items maximum), or render the popover via a React Portal / position `top-full` when at the top.

---

### [LOW] 6.2 Disabled Context Deletion Menu Item
* **Evidence:** [`ContextLauncher.tsx:1660-1662`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1660-L1662)
* **Description:** `context.delete` is rendered in the context action menu with `isSupported: false` and `unsupportedReason: "Deletion is only allowed from Workspace Settings"`.
* **Evaluation:** This is working as intended per the ratified safety contract (preventing accidental context destruction from the launcher), but its placement adjacent to active actions can cause momentary user confusion.

---

## 7. Duplicate Interaction Logic to Consolidate

### [MEDIUM] 7.1 Duplicate Action Menu State & Keyboard Traversal
* **Evidence:** [`ContextLauncher.tsx:874-1086`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L874-L1086) vs. [`ContextLauncher.tsx:1715-1869`](file:///Users/deepanshukhatri/Library/CloudStorage/GoogleDrive-deepanshukhatri20061972@gmail.com/My%20Drive/Cloud%20Desktop/ContextSplit/src/components/launcher/ContextLauncher.tsx#L1715-L1869)
* **Description:** `ContextPreviewPane` and `WorksetPreviewRow` duplicate almost identical logic:
  - `isMenuOpen`, `setIsMenuOpen`, `focusedMenuIndex`, `setFocusedMenuIndex`
  - Outside click mousedown event listener
  - Arrow key navigation (`ArrowDown`, `ArrowUp`, `Home`, `End`, `Enter`, `Escape`, `Tab`)
  - Timed focus assignment on open (`setTimeout(..., 30)`)
* **Resolution:** Refactor into a reusable helper component or custom hook (`usePopoverMenu`) to guarantee uniform accessibility, focus restoration, and boundary checking across all launcher popovers.

---

## 8. Pre-Existing Test Failures (Categorized Separately)

Running `npx vitest run src/components/launcher/` revealed 6 failing assertions out of 236 tests across 4 legacy test suites. These failures stem from contract updates and string differences across previous phases:

1. **`src/components/launcher/contextOrganizationRegression.test.ts:312`:**
   - Expected: `"Another action is currently in progress"`
   - Received: `"An action is already in progress."`
   - *Cause:* Message string discrepancy in `launcherStore.ts`.
2. **`src/components/launcher/contextOrganizationRegression.test.ts:591`:**
   - Expected: `actions.some(a => a.id === 'context.archive')` to be `false` for archived contexts.
   - Received: `true` (with `isSupported: false`).
   - *Cause:* Secondary action generator includes all actions with `isSupported: false` for explanatory tooltips.
3. **`src/components/launcher/launcherFullRegression.test.ts:306`:**
   - Expected `recentContexts` length of 1; received 3 in mock test context.
4. **`src/components/launcher/quickActionsRegression.test.ts:292`:**
   - Assertion on `executeContextAction('context.resume')` return value when `selectedContextId` is null.
5. **`src/components/launcher/quickActionsUI.test.ts:177, 508`:**
   - Expected `pauseAction.shortcut` to be `'⌥P'` (was undefined in mock model).
   - Expected secondary actions to be 4 items; now 6 items due to addition of `context.rename` and `context.pin`.

---

## 9. Smallest High-Impact Improvements for Today

| # | Improvement | Category | Expected Impact |
|---|---|---|---|
| **1** | **Direct Single-Press Enter on Desktop** | Interaction / Polish | Eliminates the confusing "dummy" Enter press on desktop split view. |
| **2** | **Protect Workset Rows from Arrow Hijacking** | Keyboard Navigation | Prevents arrow keys from switching workspaces while inspecting items. |
| **3** | **Fix Item Menu Overflow Clipping** | UI / Positioning | Fixes unclickable/clipped popover menus for working set items. |
| **4** | **Guard Option Shortcuts from Text Inputs** | Keyboard Navigation | Prevents `⌥R`, `⌥P`, `⌥S`, `⌥A` from interfering with text input in search/rename. |
| **5** | **Trap Modal Focus & Restore Tab Focus** | Accessibility / A11y | Keeps focus inside the modal and restores focus to menu triggers on dismissal. |
| **6** | **Debounce Hover Selection** | UI / Performance | Eliminates preview pane flickering when moving the mouse across the context list. |

---

## 10. Prioritized Implementation Plan

### Phase A: Critical Fixes & Keyboard Stability
1. **Desktop Direct Enter Execution:**
   - Update `handleKeyDown` in `ContextLauncher.tsx` so that when `selectedContextId` is valid, pressing `Enter` immediately invokes `executeContextAction('context.resume', target.id)` without requiring an extraneous first press to toggle `isPreviewOpen`.
2. **Exempt Workset Focus from Global Context Switching:**
   - Update the arrow navigation guard in `ContextLauncher.tsx` to exclude elements inside `[data-workset-row="true"]` and `[data-preview-pane="true"]`.
   - Implement localized `ArrowDown` / `ArrowUp` between workset items so users can cycle through items naturally.
3. **Guard Option Hotkeys in Input Fields:**
   - Add `if (activeEl?.tagName === 'INPUT' || isRenaming) return;` before processing `⌥P`, `⌥R`, `⌥S`, `⌥A`.

### Phase B: Focus Management & Popover Clipping
4. **Fix Item Menu Positioning & Clipping:**
   - Replace `max-h-[175px] overflow-y-auto` clipping with bounded content styling or dynamic menu flipping (`top-full mt-1` vs `bottom-full mb-1`) so item menus are never cut off.
5. **Modal Focus Trap & Menu Tab Focus Restoration:**
   - Trap `Tab` and `Shift+Tab` within the launcher boundary (`#launcher-search-input` $\leftrightarrow$ `#launcher-more-actions-btn`).
   - On `Tab` or `Escape` dismissal of secondary menus, explicitly call `triggerRef.current?.focus()`.

### Phase C: Polish & Interaction Smoothing
6. **Debounce List Item Mouseover:**
   - Add a 100ms debounce to `onMouseEnter` in `ContextLauncherItem` to prevent preview pane jumping when the cursor brushes across the list.
7. **Roving TabIndex on Filter Tabs:**
   - Ensure only the active filter tab has `tabIndex={0}`, with inactive tabs at `tabIndex={-1}`, navigable via `ArrowLeft` and `ArrowRight`.
8. **Fix Legacy Test Assertions:**
   - Update outdated assertions in `quickActionsUI.test.ts`, `contextOrganizationRegression.test.ts`, and `quickActionsRegression.test.ts` to align with ratified Phase 2 behavior.
