# ContextSplit

ContextSplit is a local-first, privacy-preserving desktop context intelligence engine built with Tauri, Rust, React, and TypeScript. It continuously observes and segments desktop workflows across workspaces, sessions, tasks, and memories with zero external cloud dependencies.

---

## 🛠 Prerequisites

Ensure you have the following installed on your system:

- **Node.js**: `v18.x` or higher (Node 20+ recommended)
- **pnpm / npm**: `npm v9+`
- **Rust Toolchain**: `v1.75+` (stable)
  - `rustup default stable`
  - `rustup target add <target-arch>` (e.g. `aarch64-apple-darwin` or `x86_64-apple-darwin`)
- **macOS Requirements** (if building for macOS):
  - Xcode Command Line Tools (`xcode-select --install`)
- **Windows Requirements** (if building for Windows):
  - Microsoft C++ Build Tools & WebView2 runtime

---

## 🚀 Quick Start & Development

### 1. Install Dependencies
```bash
npm install
```

### 2. Run in Development Mode
Starts the Vite frontend dev server and Tauri Rust desktop application:
```bash
npm run tauri dev
```

### 3. Run Frontend Alone (Web Preview)
```bash
npm run dev
```

---

## 🧪 Testing & Verification

ContextSplit includes comprehensive test suites across both TypeScript UI contracts and Rust backend intelligence engines:

### Frontend Unit & Contract Tests (Vitest)
```bash
npm run test
```
Executes all 68 frontend unit tests, verifying the 12 stateless `contextInteractionService` APIs, surface adapters (Notch & Windows Edge), and contract envelopes.

### Backend Context Engine Tests (Cargo)
```bash
# Run all 1,171 backend unit, integration, and E2E simulation tests
cargo test --lib --manifest-path src-tauri/Cargo.toml

# Run targeted Context Engine evaluation tests
npm run test:accuracy
```

### Accuracy Evaluation CLI
```bash
npm run eval:accuracy
```

---

## 🏗 Production Build

To produce an optimized production binary:

### 1. Build Frontend Bundle
```bash
npm run build
```
Compiles TypeScript (`tsc`) and generates the minified production client bundle via Vite into `dist/`.

### 2. Build Tauri Desktop Application Release
```bash
# Compile Rust release binary
cargo build --release --manifest-path src-tauri/Cargo.toml

# Or bundle full OS application installers (.dmg / .app / .msi)
npm run tauri build
```

---

## 🏛 Architecture Overview

ContextSplit adheres to a strict single-source-of-truth architecture:

- **Context Engine (`src-tauri/src/context_engine/`):** Central intelligence kernel managing context scoring, temporal decay, ambiguity resolution, working set derivation, task extraction, snapshot generation, and crash-safe reconstruction.
- **SQLite Database (`contextsplit.db`):** 100% local-first database running with WAL mode, foreign keys, and zero external network telemetry.
- **Capture Pipeline (`src-tauri/src/capture/`):** 9-step normalization pipeline with pre-persistence privacy filtering (excluding passwords, API keys, tokens, and password managers).
- **Stateless Interaction API (`src/services/contextInteractionService.ts`):** Frozen 12-API bridge contract separating all UI surfaces (Notch, Launcher, Windows Surface, Copilot) from internal database details.

---

## 📖 Key Documentation

- [Phase 1 Triaged Issues](docs/day-30-phase-1-triage.md)
- [Data Integrity Audit](docs/phase-1-data-integrity.md)
- [Performance Baseline](docs/phase-1-performance-baseline.md)
- [Data Safety & Privacy Audit](docs/data-safety.md)
- [Phase 1 Frozen API Contracts](docs/phase-1-api-contract.md)
- [Context Interaction Architecture](docs/context-interaction-architecture.md)
