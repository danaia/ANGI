---
name: angi-design
description: Design rules for any app, screen or page you build on ANGI. Every interface uses the Pebble Line design system (flat, faint strokes, one inverted accent) and works in both the light and dark themes. Read before you build or change a user interface.
---

# Designing for ANGI: Pebble Line

ANGI's desktop and every app on it use one design system, **Pebble Line**. When you build a new app,
page or screen (Slint, web or any other GUI), it must look as though it shipped with ANGI. Follow these
rules exactly; do not invent a new look.

## Files on this device

`/usr/share/agx/agent/skills/angi-design/assets/` holds:

- `tokens.css`: every token as CSS custom properties, with light (`:root`, `[data-theme="light"]`)
  and dark (`[data-theme="dark"]`) values.
- `tokens.json`: the same tokens with a usage note for each.
- `pebble-line.md`: principles, voice and accessibility notes.
- `web/bundle.css`, `web/bundle.js` and `web/index.d.ts`: React 18 components on `window.PebbleLine`.
- `fonts/Poppins-*.ttf`: the interface font (OFL), for offline use.

When you change the ANGI desktop itself (OS mode), the Slint versions live in the checkout:
`crates/agx-gui/ui/pebble/` (`Theme`, `Icons` and the components in `pebble.slint`), with the full rules
in `docs/design-system.md`. Read those, and the angi-gui skill.

## The look

1. **Flat.** No shadows, blur, gradients or glows. A part is either an outlined `surface` (a 1px
   `line` stroke) or a filled shape with no stroke, never both.
2. **Tokens only.** Never write a colour value in your code. Use `var(--surface)`, `var(--ink)` and so
   on (or `Theme.surface` in Slint). Ground `background`; cards and controls `surface`; wells and
   quiet selection `surface-muted`; hover `surface-hover`; strokes `line` (hover and floating layers
   `line-strong`); text `ink`, secondary `ink-muted`, disabled `ink-subtle`.
3. **One accent, inverted by theme.** `accent` is near-black in light and near-white in dark, with
   `on-accent` for text on it. Use it for the single primary action and for checked and on states.
   Fill means selected.
4. **Status stays small.** `success`, `warning` and `danger` (with `-soft` fills and `-ink` text) go on
   dots, badges and small figures, always with a word or arrow, never colour alone.
5. **Shapes.** Pill controls and buttons; cards radius 28 (`--radius-lg`); dialogs radius 40
   (`--radius-xl`); menus and wells radius 20 (`--radius-md`). Spacing on the 4-point scale (4, 8, 12,
   16, 20, 24, 32, 40, 48). Card padding 32, gaps 16–24. Lay content out as a bento of outlined tiles,
   with at most one accent tile per screen.
6. **Type: Poppins.** Weights 300, 400, 500 and 600. Heading 22/500, title 18/500, label 15/500,
   body 14/400, hint 13/400 in `ink-muted`, caption 11 uppercase in `ink-muted`. Use monospace only for
   code, paths and logs.
7. **Icons.** A 24px grid, 1.75px outline, round caps and no fills, in `currentColor`: `ink-muted` at
   rest, `ink` on hover. Icon-only buttons carry an accessible label. No emoji as icons.
8. **Voice.** Short, calm and literal. Labels are nouns, buttons are verbs, sentence case, no
   exclamation marks. Anything a person sees says ANGI.

## Light and dark

The person picks a theme in ANGI's **Settings › Appearance**. It is saved in
`~/.config/agx/appearance.conf` as `theme=dark` or `theme=light` (dark when the file is missing).

- Web: set `data-theme` on `<html>` from that file (have your server read it, or ask the person), and
  only ever use the tokens so both themes work.
- Test both themes before you report the work done. Look at the screen at a narrow (about 400px) and
  a wide window size too.

## Web app recipe

```html
<html data-theme="dark">
<head>
  <link rel="stylesheet" href="tokens.css">
  <link rel="stylesheet" href="web/bundle.css">
  <style>
    /* Offline-safe font: copy fonts/ next to your page. */
    @font-face { font-family: Poppins; font-weight: 400; src: url(fonts/Poppins-Regular.ttf); }
    @font-face { font-family: Poppins; font-weight: 500; src: url(fonts/Poppins-Medium.ttf); }
    @font-face { font-family: Poppins; font-weight: 600; src: url(fonts/Poppins-SemiBold.ttf); }
    @font-face { font-family: Poppins; font-weight: 300; src: url(fonts/Poppins-Light.ttf); }
  </style>
</head>
```

Copy the asset files into your project; do not link to the shared directory from a page. Then load
React 18, ReactDOM and `web/bundle.js`, and build with `PebbleLine.Button`, `TextField`, `Select`,
`Switch`, `SegmentedControl`, `Card`, `Badge`, `Progress`, `Dialog` and so on; see `web/index.d.ts`. Without
React, use the same tokens and the `pl-` classes from `web/bundle.css`. `bundle.css` begins with a
Google Fonts import that fails offline; the local `@font-face` rules above cover it.

## Terminal apps

Keep the terminal's own palette. Use colour sparingly, and always pair status with a word.

## Before you report the work

- [ ] Only tokens, no colour values; flat, with no shadows or gradients.
- [ ] Pebble components (or the same shapes and tokens), pill controls, one primary action.
- [ ] Checked in light and dark, narrow and wide.
- [ ] Keyboard works with a visible focus ring; icon-only controls are labelled.
- [ ] Status has words; text follows the voice rules.
