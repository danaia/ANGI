---
name: angi-desktop-tooltips
description: Add or review ANGI desktop tooltips using the Slint pattern from the Agent Angi OS and Project toggle.
---

# ANGI desktop tooltips

Use Agent Angi's OS and Project toggle in `crates/agx-gui/ui/agent-scope.slint` as the reference. Reuse `HoverDwell` and `HoverTip` from `crates/agx-gui/ui/hover-tip.slint`.

- Show the tooltip after 400 ms of continuous hover. Dismiss it on pointer exit and when the control activates.
- Use the shared Pebble Line tooltip: a solid `Theme.accent` fill with `Theme.on-accent` 12 px / 500 text, a `radius-xs` (6 px) corner, and no stroke or shadow. The shared component wraps text with 12 px horizontal and 8 px vertical padding, and it follows the light and dark themes by itself.
- Place the tooltip 8 px below the control or control group. Render it above nearby content, outside any scroll viewport, and clamp its horizontal position and width to fit the panel. Keep it passive so it cannot intercept clicks.
- Give icon-only controls a concise tooltip and an accessible label. Keep their keyboard activation and focus indicator.
