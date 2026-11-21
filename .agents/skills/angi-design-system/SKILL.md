---
name: angi-design-system
description: Design or restyle any ANGI screen or app (Slint desktop apps, dialogs, web apps served by ANGI) with the Pebble Line design system, in both the light and dark themes. Use before adding or changing UI.
---

# ANGI design system (Pebble Line)

Read [docs/design-system.md](../../../docs/design-system.md) before you design, build or restyle anything a
person sees in ANGI. It is the rulebook; this skill is the short version.

1. **Tokens only.** Colours, space, radii, strokes and type come from `Theme`
   (`crates/agx-gui/ui/pebble/tokens.slint`) or `brand/pebble-line/tokens.css`. No hex colours,
   no `.with-alpha()` tints, no font weight 700.
2. **Pebble components.** In Slint, import from `pebble/pebble.slint` (`Button`, `IconButton`,
   `TextField`, `Select`, `SegmentedControl`, `Slider`, `Switch`, `Checkbox`, `Badge`, `Progress`,
   `Card`, `Well`, `Divider`, `NavItem`, `MenuItem`, `Floating`, `Dialog`, `Alert`, `Icon`, and the type
   components `Heading2`, `Label`, `Hint` and so on). Never use std-widgets controls apart from
   `ScrollView` and `ListView`. On the web, use `window.PebbleLine` from `brand/pebble-line/web/`.
3. **Flat.** No shadows, blur or gradients. A part is an outlined `surface` or a filled shape, never
   both. Fill means selected: `accent` for the primary action and on states, `surface-muted` for
   quieter selection. Status is a small tone plus a word.
4. **Shapes.** Pills for controls, `radius-lg` cards and tiles, `radius-xl` dialogs, `radius-md`
   menus and wells. 4-point spacing (`Theme.space-*`).
5. **Icons.** `Icon { source: Icons.<name>; }`, a 24px grid with a 1.75px outline. Add new SVGs in the
   same grammar to `ui/icons/pebble/` and `pebble/icons.slint`. No Nerd Font glyphs in interface chrome.
6. **Both themes.** The person switches Light or Dark in Settings › Appearance and every token
   follows live. Check your screen in both themes and at narrow and wide window sizes before you
   report it done.
7. **Input.** Forward `pointer-moved` from a `pointer-event` handler on every interactive part (the
   device draws its own cursor). Every control works from the keyboard and shows the focus ring.
8. **Voice.** Short, calm and literal; nouns for labels, verbs for buttons, sentence case, no
   exclamation marks. Visible text says ANGI.

Finish with the checklist at the end of `docs/design-system.md`, and run the GUI tests.
