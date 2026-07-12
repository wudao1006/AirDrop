# Desktop Appearance and Floating Orb Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. The workspace is not a Git repository, so commit steps are intentionally omitted.

**Goal:** Add persistent live appearance customization, a refined compact cursor, and an optional fused-droplet desktop floating window for quick clipboard operations.

**Architecture:** A shared appearance module normalizes, persists, and applies settings through CSS variables. The main React application remains the sole owner of clipboard state and manages a secondary transparent Tauri WebviewWindow. The floating frontend uses the same entry point with a query parameter and communicates through typed Tauri events.

**Tech Stack:** React 19, TypeScript, CSS custom properties, Tauri 2 window/event APIs, Vitest, localStorage.

---

## File Structure

- Create `app/src/features/settings/appearance.ts`: appearance defaults, validation, persistence, and DOM variable application.
- Create `app/src/features/settings/AppearanceSettings.tsx`: appearance editor UI.
- Create `app/src/features/floating/floating-events.ts`: typed action/state event contracts.
- Create `app/src/features/floating/FloatingOrbManager.tsx`: main-window lifecycle and event bridge.
- Create `app/src/features/floating/floating-adapter.ts`: injectable Tauri window/event and geometry boundary.
- Create `app/src/features/floating/floating-adapter.test.ts`: lifecycle, capability-boundary, and creation-event tests.
- Create `app/src/features/floating/floating-geometry.test.ts`: scale-aware clamping and side-anchor tests.
- Create `app/src-tauri/capabilities/floating-main.json`: desktop-only main-window creation/lookup and shared event permissions.
- Create `app/src-tauri/capabilities/floating-orb.json`: least-privilege desktop-only orb event, drag, and geometry permissions.
- Create `app/src/features/floating/FloatingOrbApp.tsx`: floating surface UI, layout requests, and acknowledgement-driven rendering.
- Create `app/src/features/floating/floating.css`: fused-droplet and expanded capsule styling.
- Create `app/src/features/settings/appearance.test.ts`: normalization and persistence tests.
- Create `app/src/features/floating/FloatingOrbApp.test.tsx`: browser-safe floating surface tests.
- Modify `app/src/model.ts`: extend `AppSettings`.
- Modify `app/src/ipc/demo-client.ts`: load and persist normalized settings.
- Modify `app/src/app/App.tsx`: apply appearance variables and mount the manager.
- Modify `app/src/features/settings/SettingsPage.tsx`: render the new appearance section.
- Modify `app/src/main.tsx`: route `?surface=floating` to the orb frontend.
- Modify `app/src/styles/tokens.css`: replace customizable hard-coded alpha values with variables.
- Modify `app/src/styles/global.css`: live blur/radius/highlight variables and refined cursor.
- Modify `app/src-tauri/src/lib.rs`: exit the process when the main window closes so the orb cannot outlive it.

### Task 1: Appearance model and persistence

- [ ] Extend `AppSettings` with accent, opacity, blur, saturation, radius, highlight, and floating-orb fields.
- [ ] Implement defaults and per-field normalization in `appearance.ts`.
- [ ] Add versioned localStorage load/save helpers guarded for non-browser environments.
- [ ] Load the normalized appearance subset in `DemoDesktopClient` construction and persist only that subset in `updateSettings()`.
- [ ] Let the floating surface read the persisted subset only for initial styling before the ready/state handshake.
- [ ] Add tests for malformed input, clamping, defaults, and round-trip persistence.
- [ ] Run `npm test -- --run src/features/settings/appearance.test.ts`.

### Task 2: Live CSS variable application

- [ ] Implement `applyAppearanceSettings()` with derived accent variables.
- [ ] Replace major hard-coded window alpha, blur, saturation, radius, and highlight values with CSS variables.
- [ ] Redraw the cursor as a compact `14 × 18 px` soft-rim arrow.
- [ ] Apply appearance whenever the snapshot settings change.
- [ ] Run `npm run typecheck`.

### Task 3: Appearance settings UI

- [ ] Add color presets plus native color input.
- [ ] Add labeled range controls for opacity, Gaussian blur, saturation, radius, and highlight.
- [ ] Add reset-to-default behavior.
- [ ] Add the desktop floating-orb enable toggle.
- [ ] Verify Android hides the desktop-only toggle while retaining shared appearance controls.
- [ ] Extend render tests to cover the appearance section.

### Task 4: Floating event contract and manager

- [ ] Define typed orb actions and state payloads.
- [ ] Keep `default.json` shared and add separate desktop-only capabilities for `main` and `floating-orb`.
- [ ] Grant `main` WebviewWindow creation/lookup plus the shared event/window operations needed to manage the orb.
- [ ] Grant `floating-orb` only event listen/unlisten/emit/emit-to, start-dragging, focus observation, and the minimal geometry calls it directly performs; do not grant creation or global lookup.
- [ ] Implement browser-safe Tauri detection.
- [ ] Implement an injectable adapter and serialized, generation-token-based create/destroy reconciliation.
- [ ] Create/destroy or reuse the `floating-orb` WebviewWindow from the setting; await `tauri://created` or `tauri://error` events instead of treating the constructor as a promise.
- [ ] Listen for orb actions and map them to existing client/page actions.
- [ ] Add atomic `setSynchronizationPaused()` to the client boundary.
- [ ] Implement the ready/state handshake and broadcast typed live/paused state after snapshot updates.
- [ ] Implement typed layout request/acknowledgement events so the manager owns resize and reposition transactions.
- [ ] Revert the setting and surface an error when window creation fails.
- [ ] Test failure rollback, initial state, duplicates, rapid toggles, disable semantics, and action errors.
- [ ] Add a native close coordinator so closing `main` exits the application and destroys the orb.

### Task 5: Fused-droplet frontend

- [ ] Route `?surface=floating` in `main.tsx`.
- [ ] Implement the collapsed fused-droplet SVG/CSS silhouette.
- [ ] Implement click-to-expand capsule with clipboard, publish, sync, main-window, and hide actions.
- [ ] Add drag handling and post-drag click suppression.
- [ ] Keep the floating frontend geometry-free: emit layout requests, initiate dragging, and render only after acknowledgement.
- [ ] Implement debounced monitor-aware horizontal edge snapping, persistence, work-area clamping, and side-aware expansion inside the manager/adapter.
- [ ] Render expanded content only after a successful layout acknowledgement and fall back to collapsed on failure.
- [ ] Add live, paused, hover, and reduced-motion states.
- [ ] Add Escape, focus-loss collapse, tab order, minimum targets, and forced-colors fallback.
- [ ] Add browser-safe render tests for labels and action emission.
- [ ] Run targeted adapter and geometry tests covering browser/Android no-op, create events, rollback, StrictMode reconciliation, rapid toggles, layout acknowledgements, and clamping.

### Task 6: Integration and verification

- [ ] Run `npm run typecheck`.
- [ ] Run `npm test -- --run` and confirm all tests pass.
- [ ] Run `npm run build`.
- [ ] Run `npx vite build --mode android --outDir dist-android`.
- [ ] Build the Tauri release binary with `cargo build --bins --features tauri/custom-protocol --release --manifest-path src-tauri/Cargo.toml`.
- [ ] Replace `app/release/AirDrop` and launch with `app/release/run-wsl.sh`.
- [ ] Enable the orb, inspect collapsed/expanded states, confirm drag and actions, and verify appearance persistence after restart.
