# AirDrop Desktop UI MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a runnable Tauri desktop application that implements the approved clipboard-switching UI and a real local text clipboard bridge while keeping network synchronization behind a typed Daemon interface.

**Architecture:** React renders the main window and quick switcher from one immutable `UiSnapshot`. A small client boundary owns snapshot/revision handling and exposes commands; the initial in-process demo client makes the UI usable before the Rust Daemon exists, and all demo content is visibly labelled. Tauri owns the desktop window and local clipboard capability; no remote event can call the write command without an explicit UI import action.

**Tech Stack:** TypeScript, React, Vite, CSS custom-property design tokens, Vitest, Tauri 2, Rust, `tauri-plugin-clipboard-manager`.

---

### Task 1: Project skeleton and shared model

**Files:**
- Create: `desktop/package.json`
- Create: `desktop/tsconfig.json`
- Create: `desktop/vite.config.ts`
- Create: `desktop/index.html`
- Create: `desktop/src/main.tsx`
- Create: `desktop/src/model.ts`
- Create: `desktop/src/ipc/client.ts`
- Create: `desktop/src/ipc/demo-client.ts`

- [ ] Scaffold the Vite/React entry without a browser-oriented router.
- [ ] Define device, clipboard representation, slot availability, import state, pause state and snapshot revision types.
- [ ] Define a `DesktopClient` interface for snapshot reads, event subscriptions and explicit clipboard import commands.
- [ ] Add a clearly labelled demo client that updates device slot metadata without writing the local clipboard.
- [ ] Run `npm install` and `npm run typecheck`.

### Task 2: Visual system and application shell

**Files:**
- Create: `desktop/src/styles/tokens.css`
- Create: `desktop/src/styles/global.css`
- Create: `desktop/src/app/App.tsx`
- Create: `desktop/src/app/AppShell.tsx`
- Create: `desktop/src/components/Icon.tsx`
- Create: `desktop/src/components/StatusBadge.tsx`

- [ ] Implement semantic light/dark tokens, typography, focus rings and reduced-motion behavior.
- [ ] Implement desktop title bar, sidebar navigation and responsive content frame.
- [ ] Ensure every status uses text/icon in addition to color.
- [ ] Add loading, Daemon unavailable and empty-device states.
- [ ] Run typecheck.

### Task 3: Clipboard switcher and device slots

**Files:**
- Create: `desktop/src/features/clipboard/ClipboardSwitcher.tsx`
- Create: `desktop/src/features/clipboard/CurrentClipboard.tsx`
- Create: `desktop/src/features/clipboard/DeviceSlotCard.tsx`
- Create: `desktop/src/features/clipboard/RepresentationList.tsx`
- Create: `desktop/src/features/clipboard/ImportProgress.tsx`
- Create: `desktop/src/features/clipboard/clipboard.test.tsx`

- [ ] Render current system clipboard separately from the last published local slot.
- [ ] Render one card per origin device with ready, metadata-only, partial, stale, expired, blocked and conflict states.
- [ ] Implement search, keyboard card selection, expandable representations and device/group aggregation labels.
- [ ] Implement explicit ready import and fetching-to-awaiting-confirmation interactions.
- [ ] Verify a remote snapshot update never invokes the clipboard write command.
- [ ] Run focused component tests.

### Task 4: Main desktop pages

**Files:**
- Create: `desktop/src/features/home/HomePage.tsx`
- Create: `desktop/src/features/devices/DevicesPage.tsx`
- Create: `desktop/src/features/groups/GroupsPage.tsx`
- Create: `desktop/src/features/transfers/TransfersPage.tsx`
- Create: `desktop/src/features/settings/SettingsPage.tsx`
- Create: `desktop/src/components/Toggle.tsx`
- Create: `desktop/src/components/EmptyState.tsx`

- [ ] Build the overview from the same slot components used by the switcher.
- [ ] Build device and sync-group management surfaces with clear unavailable-backend messaging.
- [ ] Build transfer center placeholders that do not masquerade as active transfers.
- [ ] Build configurable publish/subscribe, type, preview, theme and shortcut controls.
- [ ] Run typecheck and component tests.

### Task 5: Tauri desktop integration

**Files:**
- Create: `desktop/src-tauri/Cargo.toml`
- Create: `desktop/src-tauri/build.rs`
- Create: `desktop/src-tauri/tauri.conf.json`
- Create: `desktop/src-tauri/capabilities/default.json`
- Create: `desktop/src-tauri/src/main.rs`
- Create: `desktop/src/ipc/tauri-client.ts`
- Create: `desktop/src-tauri/icons/*`

- [ ] Configure the native application window and bundled resources.
- [ ] Register clipboard-manager with the minimum clipboard permissions.
- [ ] Implement local text clipboard read/write only through explicit `confirmImport` calls.
- [ ] Select the Tauri client at runtime and keep browser preview on the labelled demo client.
- [ ] Format Rust and validate Tauri configuration where the toolchain is available.

### Task 6: Verification and handoff

**Files:**
- Create: `README.md`
- Modify: `docs/superpowers/plans/2026-07-11-desktop-ui-mvp.md`

- [ ] Run `npm run typecheck`.
- [ ] Run `npm test -- --run`.
- [ ] Run `npm run build`.
- [ ] Run `cargo fmt --check` and `cargo check` if Rust and Linux WebKit development dependencies are available; otherwise document the exact missing prerequisites.
- [ ] Document development, desktop launch and current MVP limitations without claiming network synchronization is implemented.

## Execution status

- Frontend project, shared model, desktop pages, Clipboard Switcher and Tauri configuration are implemented.
- `npm run typecheck`, `npm test -- --run`, `npm run build` and the Vite development server have been verified.
- Tauri configuration is recognized by the CLI and the clipboard plugin is registered with write-text-only frontend permission.
- Native Linux prerequisites were subsequently installed. `cargo check`, release compilation and `.deb` packaging now succeed; verified artifacts are copied to `desktop/release/`.
- Network synchronization remains intentionally unimplemented; the current client uses visibly labelled demo slots and never represents them as real discovered devices.
