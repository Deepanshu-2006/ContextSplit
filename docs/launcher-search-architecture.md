# ContextSplit Phase 2 — Launcher Context Discovery & Search Architecture

## Overview
This document specifies the architecture for **Context Discovery and Search** within the Phase 2 Context Launcher. It defines how users discover and filter existing contexts quickly, accurately, and deterministically using structured metadata already exposed by the **Phase 1 Context Interaction API** (`contextInteractionService.ts`).

---

## Architectural Principles & Invariants

```
┌─────────────────────────────────────────────────────────────┐
│                 Context Engine (Backend)                    │
│           • Single Source of Truth                          │
│           • Manages structured contexts & worksets          │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│         Context Interaction API (Stateless Bridge)          │
│           • getActiveContext()                              │
│           • getRecentContexts()                             │
│           • getContextState() / getContextWorkset()         │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│             Launcher Context Discovery Engine               │
│           • In-memory structured multi-field matching       │
│           • Zero arbitrary raw content scraping             │
│           • Zero LLM overhead / Zero secondary DB           │
│           • Sub-5ms deterministic scoring & ranking         │
└─────────────────────────────────────────────────────────────┘
```

1. **No Duplicated Database**: All context discovery operates on structured in-memory context records provided by the Context Interaction API and client workspace store. No secondary SQLite table or cache database is created.
2. **No Arbitrary Raw Content Scraping**: Discovery searches over structured metadata (context names, tasks, activity statements, domains, repositories), not raw full-text file contents or arbitrary keystroke logs.
3. **No LLM Dependencies**: Search is local, deterministic, and instant (<5ms response time), avoiding external network calls or local inference latency.
4. **Scope Isolation**: This is **Launcher Context Discovery** (Phase 2), distinct from the Phase 6 Universal Deep Search System.

---

## 1. Search Inputs

The discovery engine accepts a clean single-string input stream from the launcher's search field:

- **Raw Query String**: e.g., `"payx"`, `"auth jwt"`, `"status:active"`, `"repo:backend"`.
- **Query Preprocessing**:
  - Whitespace trimming and normalization.
  - Case-insensitive tokenization (split by whitespace into individual search terms).
  - Optional prefix token extraction (e.g., `is:active`, `is:dormant`, `tag:work`).

---

## 2. Searchable Structured Fields

The search evaluates candidate contexts across distinct structured metadata fields exposed by the Phase 1 Context Interaction API:

| Field | Source API / Contract | Description & Example | Weight Priority |
| :--- | :--- | :--- | :---: |
| **Context Name** | `Workspace.name` / `context_name` | Primary project title (e.g., `"Payment API Redesign"`, `"ContextSplit"`) | **Highest (1.00)** |
| **Active / Recent Task** | `ContextState.activeTask` / `task_title` | Inferred or confirmed active goal (e.g., `"Fix Stripe webhook timeout"`) | **High (0.80)** |
| **Recent Activity Statement** | `recent_activity` / `memory_statement` | Engine memory statement (e.g., `"Refactored token verification in auth.rs"`) | **Medium-High (0.60)** |
| **Repository / Domain Identifiers** | `WorksetItem.domain` / `app_name` | Top workset domains/apps (e.g., `"github.com"`, `"RustRover"`, `"localhost:3000"`) | **Medium (0.45)** |
| **Primary Workset Titles** | `WorksetItem.title` (Primary only) | Key file or document names in active workset (e.g., `"routes.rs"`, `"auth.ts"`) | **Medium (0.40)** |
| **Lifecycle State** | `Workspace.lifecycle_state` | State badge (`active`, `dormant`, `archived`) | **Filter / Boost** |

---

## 3. Ranking & Scoring Strategy

The discovery engine computes a composite deterministic relevance score for each candidate context:

$$\text{RelevanceScore} = \text{MatchScore} \times \text{RecencyBoost} \times \text{LifecycleWeight}$$

### 3.1 Field Matching Scores
1. **Exact Name Match**: Score = `1.0`
2. **Name Prefix / Word-Boundary Match**: Score = `0.85`
3. **Name Substring Match**: Score = `0.70`
4. **Task Title Substring Match**: Score = `0.55`
5. **Recent Activity Substring Match**: Score = `0.40`
6. **Primary Workset Item Match**: Score = `0.30`

### 3.2 Recency Boost Multiplier
- Context active within last 1 hour: $\times 1.20$
- Context active within last 24 hours: $\times 1.10$
- Context active within last 7 days: $\times 1.00$
- Older than 7 days: $\times 0.90$

### 3.3 Lifecycle State Multipliers
- `active` (Foreground/Current): $\times 1.15$
- `dormant` (Paused/Inactive): $\times 1.00$
- `archived`: $\times 0.80$ (Deprioritized unless explicitly queried)

---

## 4. Filtering Strategy

1. **Default Mode (Empty Query)**:
   - Top item: Current active context (with pulse indicator).
   - Subsequent items: Recent contexts strictly sorted by Context Engine's chronological work-sequence order.
2. **Query Mode (Non-Empty Query)**:
   - Filters candidate contexts where $\text{MatchScore} > 0$.
   - Orders strictly by $\text{RelevanceScore}$ descending.
   - Dedupes identical context IDs.
   - Auto-selects the top-ranked match.
3. **Special Filtering Directives**:
   - `is:active` $\rightarrow$ Filters to active/open workspaces.
   - `is:dormant` / `is:archived` $\rightarrow$ Filters by lifecycle state.

---

## 5. UI States & Interaction Specifications

### 5.1 Empty State
- **When 0 Contexts Exist in DB**:
  - Message: *"No contexts yet. Start working and ContextSplit will build your context."*
- **When Search Matches 0 Contexts**:
  - Title: *"No contexts matching '<query>'"*
  - Subtitle: *"Try searching with a different keyword or check spelling."*
  - Action shortcut: `Esc` to clear search or close.

### 5.2 Loading State
- **Initial Fetch**: Compact skeleton shimmer rows for list and preview pane without layout shift or UI freezing.
- **Search Latency**: Filter execution runs synchronously in memory (<2ms), guaranteeing zero input lag or cursor freezing.

### 5.3 Error State
- If Context Interaction API returns an IPC communication error:
  - Displays a compact, non-intrusive error banner with a `Retry` action.
  - Retains already loaded in-memory context records.

---

## 6. Keyboard Behavior

Discovery and search integrate seamlessly into the Mac keyboard interaction model:

| Key Binding | Action |
| :--- | :--- |
| **`⌘F` / `⌃F` / `/`** | Instant focus on search input. |
| **`↓` / `⌃N`** | Move selection to next matching context. |
| **`↑` / `⌃P`** | Move selection to previous matching context. |
| **`⌘↑` / `Home`** | Jump to top-ranked search result. |
| **`⌘↓` / `End`** | Jump to bottom-ranked search result. |
| **`↵` / `Return`** | Resume highlighted context. |
| **`Esc`** | If search has text $\rightarrow$ Clears search query and refocuses input. If empty $\rightarrow$ Closes launcher. |

---

## 7. Future Extension Point: Semantic Search (Phase 6 Alignment)

While Phase 2 Context Discovery relies on fast, structured deterministic matching, the architecture provides clean extension hooks for Phase 6 Universal Deep Search:

```typescript
export interface IContextSearchProvider {
  /** Phase 2 Fast In-Memory Structured Discovery */
  searchStructured(query: string, contexts: LauncherContextItem[]): LauncherContextItem[];
  
  /** Phase 6 Semantic Vector / Deep Search Extension Hook */
  searchSemantic?(query: string): Promise<LauncherContextItem[]>;
}
```

- When Phase 6 introduces deep item embeddings, the search pipeline can blend semantic similarity scores into `MatchScore` without breaking the launcher UI contract or requiring rewrites of the presentation layer.

---

## 8. Summary

The Context Discovery architecture provides a high-velocity, deterministic, and distraction-free search experience for the Context Launcher. By searching over high-signal structured fields already exposed by Phase 1 APIs, it maintains sub-5ms responsiveness while strictly respecting architectural boundaries.
