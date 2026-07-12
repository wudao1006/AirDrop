# Desktop Appearance Customization and Floating Orb Design

## Goal

Add a persistent desktop appearance editor, replace the current cursor with a refined compact pointer, and provide an optional always-on-top floating orb for fast clipboard actions.

## Scope

- Desktop floating orb only. Android does not create a system overlay.
- The existing clipboard behavior remains explicit: remote content is never written until the user confirms it.
- The floating orb exits with the application; background persistence is out of scope.
- Appearance settings persist locally across restarts.

## Visual Direction

The floating control uses a **fused droplet** rather than a generic circle. Two offset translucent lobes form a soft asymmetric `52 × 48 px` silhouette. A pair of internal curved strokes represents content flowing between devices. Cyan light enters from the upper-left and blue refraction gathers at the lower-right.

The orb expands horizontally into a rounded liquid-glass action capsule. Hover causes a subtle parallax shift between the two lobes; active synchronization animates the inner flow lines slowly. When dragged near a monitor edge, the orb snaps to the nearest edge while remaining fully accessible.

The cursor becomes a compact `14 × 18 px` macOS-like arrow with a dark teal body, soft white rim, and round joins. It must remain legible on light and dark content without looking heavier than the native pointer.

## Appearance Settings

`UiSnapshot.settings` remains the single application source of truth. Extend `AppSettings` with:

- `accentColor`: custom CSS hex color.
- `windowOpacity`: `0.72–1.0`.
- `blurStrength`: `12–56 px`.
- `glassSaturation`: `0.9–1.6`.
- `cornerRadius`: `14–30 px`.
- `highlightStrength`: `0–0.6`.
- `floatingOrbEnabled`: desktop-only boolean.

The settings page adds a dedicated “外观与液态玻璃” panel with a color input, compact preset swatches, range controls with numeric output, a floating-orb toggle, and “恢复默认外观”. Changes apply immediately through CSS custom properties.

Only the appearance subset (`theme` plus the new fields above) is persisted as versioned JSON in local storage. Clipboard policy and privacy fields remain session-owned until the Rust daemon exists. `DemoDesktopClient` loads the normalized appearance subset into `UiSnapshot.settings` during construction and persists that subset inside `updateSettings()`. The floating window may read the same persisted appearance only for its own initial styling; live behavior and status always come from main-window events.

## Runtime Appearance Architecture

Create a focused appearance module that owns defaults, validation, persistence, and DOM application. The UI and demo client consume the same normalized settings rather than independently interpreting raw values.

`App` applies normalized settings to the root document through these variables:

- `--accent`
- `--accent-hover`
- `--accent-soft`
- `--accent-text`
- `--window-opacity`
- `--glass-blur`
- `--glass-saturation`
- `--user-radius`
- `--glass-highlight-opacity`

The theme CSS derives component values from those variables. Hard-coded major blur, radius, and accent values are replaced only where required for live customization.

## Floating Window Architecture

The main window remains the only owner of clipboard state. A desktop-only `FloatingOrbManager` observes `floatingOrbEnabled` and creates or destroys a transparent Tauri `WebviewWindow` labeled `floating-orb`.

The floating window loads the same frontend entry with `?surface=floating`. `main.tsx` routes that query to a small `FloatingOrbApp` instead of the full application. This avoids a second build entry and keeps shared styling available.

Window properties:

- transparent and undecorated;
- always on top;
- hidden from the taskbar;
- non-resizable;
- initial size approximately `72 × 68 px`;
- expanded size approximately `300 × 76 px`;
- draggable using the Tauri drag region;
- closes automatically with the process.

## Inter-window Commands

The floating frontend emits a single typed event, `airdrop://orb-action`, with one of:

- `open-main`
- `open-clipboard`
- `publish-current`
- `toggle-sync`
- `hide-orb`

Expansion is a separate geometry transaction. The floating frontend emits `airdrop://orb-layout` with `{ expanded: boolean }`. The main-owned geometry manager computes the anchored size and position, applies resize and reposition as one serialized operation, then replies directly with `airdrop://orb-layout-state` containing `{ expanded, success, message? }`. The orb changes its rendered layout only after a successful acknowledgement. On failure it stays or returns collapsed and exposes an accessible status message without blocking the core droplet action.

The main window listens and performs the action using its existing client instance. Opening the main window also shows, unminimizes, and focuses it. `open-clipboard` switches the React page to `clipboard`.

Add an atomic `setSynchronizationPaused(paused)` client method. `toggle-sync` pauses both directions when either direction is currently active, and resumes both only when both are paused. The client mutates both flags before emitting one snapshot, preventing a half-paused intermediate state. Errors continue through the existing toast path.

The orb emits `airdrop://orb-ready` after its listeners are installed. The main window responds directly with `airdrop://orb-state`, removing the startup race, and also rebroadcasts after snapshot changes. The state payload contains `publishPaused`, `subscribePaused`, `activity`, `canReadClipboard`, `busy`, and the normalized appearance subset. All listeners are removed during React cleanup and window destruction.

## Tauri Capabilities

The capability configuration explicitly covers both `main` and `floating-orb`. The main window receives WebviewWindow creation and lookup permissions. Both windows receive the minimum required event and window permissions: show, hide, close, focus, unminimize, start dragging, get/set size, get/set position, monitor lookup, and scale factor. Android never executes or receives these desktop capabilities.

## Interaction Details

- Click the collapsed droplet to expand.
- Losing window focus, pressing Escape, completing an action, or waiting briefly after an action collapses the capsule. A WebviewWindow cannot observe arbitrary clicks elsewhere on the desktop.
- Dragging does not trigger expansion.
- The expanded capsule offers: clipboard, publish, pause/resume, main window, and disable. “Disable” updates `floatingOrbEnabled=false` and closes the window; it is not a temporary unreachable hide state.
- Enter/Space expands the orb. Escape collapses it. Expanded actions follow a predictable tab order with at least `36 × 36 px` targets. The drag handle never overlaps action buttons, and focus returns to the droplet after collapse.
- Reduced-motion mode disables parallax, morph, and flow animation.
- Forced-colors mode and cursor-image failure fall back to the native system cursor and solid system borders.

## Geometry and Reconciliation

Window lifecycle reconciliation is serialized and idempotent so React StrictMode and rapid setting changes cannot create duplicates or resurrect a disabled orb. Creation first looks up `floating-orb`; an existing instance is reused. A generation token invalidates stale asynchronous creation results.

The manager owns geometry. It listens to moved events and debounces snapping until movement stops. Physical positions and sizes are converted using the current monitor scale factor. The collapsed orb snaps to the nearest horizontal work-area edge and clamps its vertical position inside the monitor work area. Its last monitor-relative side and vertical fraction are persisted separately from appearance settings.

Before expansion, the manager determines the anchored side. A right-edge orb grows left; a left-edge orb grows right. Resize and reposition happen together and are clamped to the current work area. Monitor and DPI changes trigger a fresh clamp. If monitor APIs or positioning are unsupported, the fallback keeps the window movable without snapping and never blocks core actions.

## Error Handling

- Failure to create the floating window reverts `floatingOrbEnabled` and reports an actionable toast.
- Duplicate-window creation focuses the existing orb instead of creating another.
- Malformed persisted appearance settings are ignored field by field.
- Closing the main window terminates the process and therefore the orb; no background-resident state is retained.
- Tauri-only calls are guarded so browser tests and Android builds remain functional.

## Verification

- Unit-test appearance normalization and persistence behavior.
- Render-test the new settings controls and reset action.
- Render-test floating surface routing without requiring Tauri internals.
- Test an injected floating-window adapter for browser/Android no-op behavior, create failure rollback, ready/state handshake, StrictMode duplicate reconciliation, rapid enable-disable changes, disable semantics, action error propagation, and geometry clamping.
- Test layout request acknowledgement, side-aware resize/reposition, and failure fallback to the collapsed state.
- Preserve the existing eight clipboard tests.
- Run typecheck, Vitest, desktop build, and Android build.
- Build the Tauri custom-protocol release binary, launch it under WSL, enable the orb, and visually inspect both collapsed and expanded states.
