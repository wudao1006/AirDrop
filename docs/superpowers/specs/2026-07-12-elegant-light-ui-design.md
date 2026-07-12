# Elegant Lightweight UI Design

## Direction

The approved direction combines a quiet native utility with restrained glass surfaces. Native restraint controls hierarchy, density and interaction; translucency is limited to the navigation rail, mobile status bar and Clipboard Switcher toolbar. Ordinary content surfaces remain opaque and cheap to render.

## Visual principles

- Use one low-saturation indigo accent. Status colors are reserved for actual state and use small dots or quiet labels.
- Remove dashboard-style metric cards, promotional greetings, oversized headings and nested card borders.
- Use a soft neutral canvas, opaque content surfaces, 1px borders and very shallow shadows.
- Standard radii are 10px for controls, 12px for rows and 16px for floating glass surfaces.
- Typography uses compact 13–14px body text, 20–24px page titles and no all-caps marketing labels.
- Motion is limited to 120–160ms opacity, background and small translate transitions. Reduced-motion disables them.
- Glass uses a single translucent layer with blur. It is never nested inside another blurred surface.

## Desktop structure

- Navigation rail is 204px, visually quiet and separated by a hairline border.
- Brand is reduced to a 30px mark and product name; “Local flow” marketing copy is removed.
- Home becomes a utility overview: compact header actions, current clipboard, then latest device slots.
- Clipboard page becomes the primary working surface. The right-side statistics card is removed.
- Device slots are flat list rows with generous horizontal rhythm. The selected row receives a subtle accent tint instead of a heavy shadow.
- Ready/import actions are compact and persistent. Secondary details expand inline.
- Demo mode is shown as a thin neutral information strip, not a large warning panel.

## Android structure

- Bottom navigation contains Home, Clipboard, Devices and Settings. Groups and Transfers are reached from relevant pages rather than occupying permanent tabs.
- The Clipboard Switcher is full-width and uses the same flat rows.
- Touch targets remain at least 44px; glass is limited to the sticky top and bottom bars.
- Foreground lifecycle state is a compact inline indicator rather than a full-width colored block unless action is required.

## Component behavior

- `StatusBadge` becomes a quiet status label with an optional 6px dot.
- `CurrentClipboard` removes its inset preview card and uses a simple text block with a thin divider.
- `DeviceSlotCard` loses card shadows and uses row separators; safety conflicts retain a persistent danger tint.
- Buttons have flat secondary treatment; only the immediate primary action uses a filled accent.
- Empty states are smaller and avoid oversized illustration containers.

## Performance boundaries

- No image backgrounds, icon fonts, remote fonts or heavyweight component library.
- Blur appears on at most three fixed/sticky surfaces and is disabled through a fallback when unsupported.
- Existing inline SVG icons, CSS tokens and shared React components remain.
- No additional runtime dependency is required for the redesign.

## Acceptance

- The home page contains no metric-card dashboard row or promotional greeting.
- Desktop and Android navigation remain fully keyboard/touch accessible.
- All existing Import, lifecycle and clipboard behavior tests continue to pass.
- Desktop and Android-mode production bundles do not materially grow from the current baseline.
- The Linux native program compiles and launches with the redesigned assets.
