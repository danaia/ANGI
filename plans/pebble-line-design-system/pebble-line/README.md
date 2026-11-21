# Pebble Line

Pebble Line is the flat sibling of Pebble: the same shapes (pills, large radii, bento tiles), the same type and components, with every shadow removed. Edges are faint 1px strokes, and emphasis comes from fill and weight, never depth. It ships in a light and a dark theme.

Built from Pebble, which was built from four neumorphism references (a bento template, a minimal login form, shop components). The component API is identical except for variant names (see *Moving from Pebble*). There is no logo; the name is set in Poppins.

## Principles

1. **Flat.** No `box-shadow`, no blur, no gradients. A part is a `surface` shape with a 1px `line` stroke, or a filled shape with no stroke — never both a stroke and a fill tint.
2. **Faint strokes.** `line` is barely there (≈1.3:1). It suggests a container; it doesn't shout one. Strokes darken one step to `line-strong` on hover and for floating layers (menus, dialogs, toasts), since nothing else separates them from the page.
3. **Fill means selected.** The current page, the checked box, the on switch and the primary action are solid `accent`. Quieter selection (current nav item, selected segment) uses `surface-muted`.
4. **One accent, inverted by theme.** `accent` is near-black in light and near-white in dark, so the primary pill is always the highest-contrast shape on screen. Status colours stay small: dots, icon discs, delta text.
5. **Same shapes.** Pills for controls, `radius-lg`/`radius-xl` for cards, bento layouts. The gap tightens to `space-6` because there are no shadows to clear.

## Voice

Short, calm, literal. Labels are nouns ("Username", "Price Range"); buttons are verbs ("Login", "Apply", "Checkout"). Uppercase only for small captions under a rule. No exclamation marks, no emoji.

## Colour

Every colour token has a light and a dark value; components only reference tokens, so switching `data-theme` switches everything.

| Role | Tokens |
| --- | --- |
| Ground | `background` (page), `surface` (cards, controls), `surface-muted` (wells, selected), `surface-hover` |
| Strokes | `line` (default 1px), `line-strong` (hover, floating layers), `line-bold` (checkbox and radio rings, 3:1+) |
| Text | `ink`, `ink-muted`, `ink-subtle` (disabled only) |
| Accent | `accent` + `on-accent`, `accent-hover`, `accent-soft` (neutral badge and avatar fill) |
| Status | `success` / `warning` / `danger`, each with `-soft` fill and `-ink` text |
| Overlay | `scrim` — flat, no blur |

## Strokes

| Token | Width | Used for |
| --- | --- | --- |
| `stroke-hair` | 1px | Every outline and divider |
| `stroke-rule` | 1.5px | Heading underline, summary total, current tab, checkbox and radio rings |
| `stroke-focus` | 2px | Focus ring |

Field focus is the one place a stroke gets heavy: the 1px outline turns `ink` and gains a second 1px ring.

## Type

Poppins (Google Fonts, 300/400/500/600), exactly as in Pebble: `display` 56/600, `display-light` 44/300, `numeral` 32/300 tabular for figures, `heading-1` 32/600, `heading-2` 22/500, `title` 18/500, `label` 15/500, `body` 14/400, `body-sm` 13/400, `caption` 11/400 uppercase.

## Shape and space

Radii: cards `radius-lg` (28), hero cards and dialogs `radius-xl` (40), controls `radius-pill`, wells `radius-md` (20), nav items `radius-sm` (12), checkboxes `radius-xs` (6). Spacing 4·8·12·16·20·24·32·40·48 (`space-1` … `space-12`). Card padding `space-8`, hero `space-10`, bento gap `space-6`.

## Iconography

`Icon`: 24px grid, 1.75px outline, round caps, `currentColor` — the same weight as the UI's strokes. `ink-muted` at rest, `ink` on hover, `on-accent` inside filled shapes. Icon-only buttons always carry a `label`.

## Layout: bento

Outlined tiles in unequal spans (`BentoGrid` + `BentoCell`). Mix one tall or wide tile per row and fill at most one with `accent`. Nest a `muted` Card inside an outline Card for read-only groups — never outline inside outline (double strokes).

## Accessibility

- Text passes AA in both themes: `ink` is 11.6:1 or better on every surface; `ink-muted` is 5.3:1+ in light and 6.0:1+ in dark.
- `on-accent` on `accent`: 12.6:1 light, 15.3:1 dark.
- **Flagged: faint strokes.** `line` (≈1.3:1) and `line-strong` (≈1.8:1) are below the 3:1 non-text guideline. That is the requested look. Labels are always visible; checkbox and radio rings use `line-bold` (3:1+); every selected state changes fill, not only stroke; focus is a 2px `focus` outline. For strict-contrast products, swap `line` on `.pl-control` and `.pl-btn--secondary` for `line-bold`.
- Status never relies on colour alone: deltas carry arrows; badges take a dot and a word.

## Moving from Pebble

Same components and props, with flat variant names: Button `raised` → `secondary`; Card `raised` → `outline`, `inset` → `muted`, `graphite` → `accent`; StatTile the same; Badge tone `graphite` → `accent`. The namespace is `window.PebbleLine`; classes are prefixed `pl-`. Token names differ: `surface` → `background` for the page, and the shadow family is replaced by `stroke`.

## Using the components

`components/bundle.js` defines `window.PebbleLine` (React 18 on the page). Load `tokens.css`, then `components/bundle.css` (which imports Poppins), then React, ReactDOM and the bundle. Set `data-theme="dark"` on `<html>` for the dark theme. Inputs are uncontrolled by default (`defaultValue`, `defaultChecked`) and controlled when you pass `value` / `checked`. `LoginExample` and `ShopExample` show the reference screens rebuilt flat.
