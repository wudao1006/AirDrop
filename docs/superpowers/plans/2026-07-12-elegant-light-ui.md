# Elegant Lightweight UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the shared desktop/Android UI as a quiet native utility with restrained glass framework surfaces and flat, compact content rows.

**Architecture:** Existing behavior, IPC and state models remain unchanged. The work removes dashboard structure from feature components, simplifies shared clipboard components and replaces the current visual tokens/CSS hierarchy without adding dependencies.

**Tech Stack:** React, TypeScript, CSS custom properties, Tauri 2, Vitest.

---

### Task 1: Navigation and page hierarchy

**Files:**
- Modify: `app/src/app/AppShell.tsx`
- Modify: `app/src/features/home/HomePage.tsx`
- Modify: `app/src/features/clipboard/ClipboardPage.tsx`
- Test: `app/src/features/clipboard/clipboard.test.tsx`

- [ ] Reduce desktop brand and navigation chrome.
- [ ] Reduce Android bottom navigation to Home, Clipboard, Devices and Settings.
- [ ] Remove the home metric dashboard and promotional greeting.
- [ ] Remove Clipboard page side statistics and keep one primary work surface.
- [ ] Update render assertions for the new compact headings.

### Task 2: Clipboard component simplification

**Files:**
- Modify: `app/src/features/clipboard/CurrentClipboard.tsx`
- Modify: `app/src/features/clipboard/DeviceSlotCard.tsx`
- Modify: `app/src/features/clipboard/ClipboardSwitcher.tsx`
- Modify: `app/src/features/clipboard/ImportProgress.tsx`
- Modify: `app/src/components/StatusBadge.tsx`

- [ ] Convert nested cards to flat rows and separators.
- [ ] Use quiet status dots/labels and reserve strong color for conflicts.
- [ ] Keep all Import actions and accessibility labels intact.
- [ ] Convert demo warning into a compact neutral information strip.

### Task 3: Visual tokens and restrained glass

**Files:**
- Modify: `app/src/styles/tokens.css`
- Modify: `app/src/styles/global.css`
- Modify: `app/src/features/clipboard/clipboard.css`
- Modify: `app/src-tauri/tauri.conf.json`

- [ ] Replace saturated surfaces and large shadows with neutral tokens.
- [ ] Apply blur only to navigation, mobile bars and switcher toolbar.
- [ ] Standardize compact typography, spacing, radii and 120–160ms motion.
- [ ] Preserve high contrast, reduced motion and 44px Android touch targets.
- [ ] Tune the native window size for the denser layout.

### Task 4: Verification and native rebuild

- [ ] Run TypeScript typecheck and all tests.
- [ ] Build desktop and Android-mode frontend assets and compare sizes.
- [ ] Run Cargo check and release compilation.
- [ ] Copy the redesigned native binary to `app/release/AirDrop`.
- [ ] Launch through the WSL compatibility script for user inspection.

## Execution status

- Home dashboard metrics, promotional greeting and Clipboard side statistics were removed.
- Desktop navigation was quieted; Android navigation now contains four permanent destinations.
- Clipboard surfaces use flat rows, subtle selection tint and a single glass toolbar layer.
- Neutral light/dark tokens, compact typography and restrained 140ms interaction transitions are active.
- All 8 behavior tests pass; desktop and Android-mode assets build successfully.
- The redesigned Linux native binary compiles to 4.9 MiB and is running through `app/release/run-wsl.sh` for inspection.
