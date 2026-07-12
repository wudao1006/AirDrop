# Desktop and Android Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the desktop Tauri MVP into a shared desktop/Android application model with foreground-only Android synchronization, touch-native navigation and explicit current-clipboard publication.

**Architecture:** The React feature layer stays shared while `PlatformKind` and `AppActivity` make platform differences explicit. A platform-neutral client receives visibility changes, owns reconnect/Snapshot semantics and exposes clipboard publication commands. Desktop retains sidebar/tray-oriented concepts; Android renders touch navigation and foreground-state guidance without claiming background persistence.

**Tech Stack:** TypeScript, React, Vite, Vitest, Tauri 2, Rust, Tauri clipboard-manager.

---

### Task 1: Product scope and architecture documentation

**Files:**
- Modify: `DESIGN.md`
- Modify: `README.md`
- Reference: `docs/superpowers/specs/2026-07-11-desktop-android-design.md`

- [ ] Replace Android/iOS combined non-goal with Android foreground-client scope and explicit iOS exclusion.
- [ ] Add Android process, lifecycle, clipboard capability, UI and acceptance sections.
- [ ] Document required Java/SDK/NDK/Rust targets and current environment limitations.

### Task 2: Platform and lifecycle model

**Files:**
- Modify: `app/src/model.ts`
- Modify: `app/src/ipc/client.ts`
- Modify: `app/src/ipc/demo-client.ts`
- Modify: `app/src/ipc/tauri-client.ts`
- Create: `app/src/app/useAppLifecycle.ts`
- Test: `app/src/features/clipboard/clipboard.test.tsx`

- [ ] Add `desktop | android`, `foreground_live | reconnecting | suspended` and clipboard capability types.
- [ ] Detect Android without coupling feature components to user-agent parsing.
- [ ] Propagate document visibility to the client and force reconnect/Snapshot semantics on foreground resume.
- [ ] Test suspension, resume and no clipboard writes during lifecycle changes.

### Task 3: Android-adaptive application shell

**Files:**
- Modify: `app/src/app/App.tsx`
- Modify: `app/src/app/AppShell.tsx`
- Modify: `app/src/styles/global.css`
- Modify: `app/src/features/home/HomePage.tsx`
- Modify: `app/src/features/clipboard/ClipboardPage.tsx`
- Modify: `app/src/features/clipboard/clipboard.css`
- Modify: `app/src/features/settings/SettingsPage.tsx`

- [ ] Add Android bottom navigation and safe-area spacing while retaining desktop sidebar behavior.
- [ ] Add foreground-live, suspended and reconnecting indicators.
- [ ] Replace hover-dependent actions with persistent touch controls and 44px targets on Android.
- [ ] Hide desktop-only shortcut/tray language on Android.
- [ ] Add render tests for Android navigation and state messaging.

### Task 4: Explicit current clipboard publication

**Files:**
- Modify: `app/src/ipc/client.ts`
- Modify: `app/src/ipc/demo-client.ts`
- Modify: `app/src/ipc/tauri-client.ts`
- Modify: `app/src/features/home/HomePage.tsx`
- Modify: `app/src/features/settings/SettingsPage.tsx`
- Modify: `app/src-tauri/capabilities/default.json`
- Test: `app/src/features/clipboard/clipboard.test.tsx`

- [ ] Add a clipboard reader only used by an explicit foreground user command.
- [ ] Publish the read text as the local latest slot without changing remote slots.
- [ ] Surface denied/unavailable clipboard capability without claiming capture is active.
- [ ] Grant Tauri read-text permission and keep write-text permission.
- [ ] Test that lifecycle changes never read and a publication command reads exactly once.

### Task 5: Verification

- [ ] Run `npm run typecheck`.
- [ ] Run `npm test -- --run`.
- [ ] Run `npm run build`.
- [ ] Run `npm run tauri info` and record missing Android/native prerequisites honestly.
- [ ] Confirm iOS has no generated project, dependency or claimed support.

## Execution status

- Desktop and Android shared platform/lifecycle models are implemented.
- Android touch navigation, safe-area layout, foreground status and suspended-action guards are implemented.
- Explicit current-text clipboard publication is available on desktop and Android, including retry after a denied read.
- Snapshot application now rejects stale revisions, fixing a resume race where an older initial Snapshot could overwrite a newer live event.
- Typecheck passes, all 8 tests pass, and both desktop and Android-mode production assets build successfully.
- Tauri Android commands are present, but native Android initialization/APK generation is not possible on this host because Java and Android SDK/NDK are absent. Rust and the Linux desktop dependencies were subsequently installed; desktop release compilation and `.deb` packaging now succeed.
- No iOS project or dependency was generated.
