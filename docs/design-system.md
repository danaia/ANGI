# ANGI design system: Pebble Line

Every part of ANGI a person sees uses **Pebble Line**: the desktop shell, built-in apps, dialogs, and
any new app an agent builds. This page holds the rules. Follow them exactly when you design or change a
screen; do not invent a new look.

Pebble Line is flat. There are no shadows, blur or gradients. Surfaces are outlined with faint
1px strokes, emphasis comes from fill and weight, and there is a single accent that inverts with the
theme. ANGI ships a **light** and a **dark** theme. A person chooses one in **Settings › Appearance**,
and every screen must look right in both.

## Where things live

| What | Path |
| --- | --- |
| Token source of truth (colours for both themes, space, radius, stroke, type) | `brand/pebble-line/tokens.json` |
| The same tokens as CSS custom properties, for web apps | `brand/pebble-line/tokens.css` |
| Pebble Line reference: principles, voice and accessibility notes | `brand/pebble-line/README.md` |
| Web components (`window.PebbleLine`, React 18) | `brand/pebble-line/web/bundle.{css,js}` and `index.d.ts` |
| Slint tokens (`Theme`) | `crates/agx-gui/ui/pebble/tokens.slint` |
| Slint icons (`Icons`) | `crates/agx-gui/ui/pebble/icons.slint`, SVGs in `ui/icons/pebble/` |
| Slint components | `crates/agx-gui/ui/pebble/{type,actions,inputs,surfaces}.slint`, imported through `pebble/pebble.slint` |
| Theme preference (per account, `~/.config/agx/appearance.conf`) | `crates/agx-gui/src/appearance.rs` |
| Fonts (Poppins 300/400/500/600, JetBrains Mono Nerd Font) | `crates/agx-gui/ui/fonts/` |

## The rules

### 1. Tokens only

- Never write a hex colour, `rgb()` or `.with-alpha()` tint in a feature file. Use a `Theme` token
  (Slint) or a `var(--token)` (web). If no token fits, the design is wrong; ask, or add a token to
  `tokens.json`, `tokens.css` and `tokens.slint` together, with values for **both** themes.
- Use the role, not the look: `surface` for a card, `ink-muted` for secondary text, `danger` for a
  destructive action. Never pick a token because its current colour happens to match.
- Allowed exceptions, each already commented in the code: the terminal cell grid
  (`Theme.terminal-*`, always dark because the ANSI palette needs it), the literal swatches in the
  Appearance page's theme previews, the cursor bitmaps, and web-page pixels.

### 2. Colour roles

| Role | Tokens | Use |
| --- | --- | --- |
| Ground | `background` | Behind windows (the desktop) and behind cards in a web page |
| Surface | `surface` | Windows, cards, controls, menus, dialogs |
| Wells and selection | `surface-muted` | Read-only groups, slider rails, the current nav item, selected rows, segmented troughs |
| Hover | `surface-hover` | Row, menu item and ghost-button hover |
| Strokes | `line` (default 1px), `line-strong` (hover; floating layers), `line-bold` (checkbox and radio rings) | Outlines and dividers |
| Text | `ink`, `ink-muted` (descriptions, captions), `ink-subtle` (disabled only) | All text |
| Accent | `accent`, `accent-hover`, `on-accent` (text and icons on accent), `accent-soft` | The one primary action, checked and on states. Near-black in light, near-white in dark |
| Status | `success` / `warning` / `danger`, each with `-soft` (fill) and `-ink` (text on that fill) | Dots, badges, small figures. Never large areas |
| Overlay | `scrim` | Flat backdrop behind a modal |
| Focus | `focus` | The 2px keyboard focus ring |

- **Fill means selected.** The current page, the checked box, the on switch and the primary action are
  solid `accent`. Quieter selection (current nav item, selected row, chosen segment) uses
  `surface-muted`.
- **Outline or fill, never both.** A part is either a `surface` shape with a 1px `line` stroke, or a
  filled shape with no stroke. Never nest an outlined card inside another outlined card; nest a
  `Well` or a `muted` `Card` instead.
- **Status is never colour alone.** Pair a tone with a dot and a word (`Badge { tone: Tone.warning;
  dot: true; text: "30/100 · Moderate"; }`), or with an arrow for deltas.
- **No per-app colours.** App icons are `ink` outlines in a `surface-muted` well. Do not tint by
  category.

### 3. Type

Poppins is the interface font (the window's default). Use the Slint type components from
`pebble.slint` rather than raw sizes:

| Component | Size / weight | Use |
| --- | --- | --- |
| `Display` | 56 / 600 | Hero figure only |
| `Numeral` | 32 / 300 | Big numbers on stat tiles |
| `Heading1` | 32 / 600 | Page title in a large app |
| `Heading2` | 22 / 500 | Page or dialog heading ("Mouse & pointer") |
| `Title` | 18 / 500 | Card title, compact dialog title |
| `Label` | 15 / 500 | Setting names, list primaries |
| `Body` | 14 / 400 | Running text |
| `Hint` | 13 / 400, `ink-muted` | The description under a label |
| `Caption` | 11 / 400, uppercase, `ink-muted` | Small group captions ("PREFERENCES") |
| `Mono` | 13, JetBrains Mono | Paths, logs, code and terminal output only |

- Weights are 300, 400, 500 and 600. Never use 700; Poppins Bold is not shipped.
- If you need a raw size, use the `Theme.text-*` tokens.
- Content written by a person or a model that may contain any script (chat messages, file contents)
  uses Noto Sans (`AgentText`). Controls and labels never do.

### 4. Shape, space and stroke

- Radii: controls and buttons are pills (`height / 2`); window tiles and cards `radius-lg` (28);
  dialogs and the login card `radius-xl` (40); wells, menus and popovers `radius-md` (20); nav items
  are pills; small chips `radius-sm` (12); checkboxes and menu rows `radius-xs` (6).
- Spacing is the 4-point scale only: `space-1` 4, `space-2` 8, `space-3` 12, `space-4` 16,
  `space-5` 20, `space-6` 24, `space-8` 32, `space-10` 40, `space-12` 48. Window content pads
  `space-6` (16 in narrow windows); dialogs pad `space-8`; the login card pads `space-10`.
- Strokes: `stroke-hair` (1px) for every outline and divider; `stroke-rule` (1.5px) for the focused
  window (the `focus-stroke` fade when open, solid `ink-muted` when collapsed), the current tab underline, and checkbox and radio rings; `stroke-focus` (2px) for the
  focus ring.

### 5. Components (Slint)

Import from `pebble/pebble.slint`. Do not use `std-widgets` `Button`, `LineEdit`, `ComboBox`,
`Slider`, `CheckBox`, `Switch` or `ProgressIndicator`; their style does not follow the theme.
`ScrollView` and `ListView` are fine.

| Need | Use |
| --- | --- |
| Action | `Button` (`variant`: `primary` for the single main action, `secondary` (default), `ghost` for quiet actions, `danger` for destructive ones; `size`: `sm` 36, `md` 44, `lg` 56) |
| Icon-only action | `IconButton` with a `label`; `quiet: true` in dense chrome |
| Text entry | `TextField` (pill, optional leading `icon`) |
| One of many, 2–5 short options | `SegmentedControl` |
| One of many, longer lists | `Select` |
| Number in a range | `Slider` |
| On/off with immediate effect | `Switch` |
| Opt-in in a form | `Checkbox` |
| Status word | `Badge` (`tone`, `dot`) |
| Amount | `Progress` (accent bar; a status `tone` only when the value is itself a warning) |
| Container | `Card` (`outline`, `muted`, `accent`), `Well`, `Divider`, `VDivider` |
| Sidebar navigation | `NavItem` |
| Menus and popovers | `Floating` with `MenuItem`s |
| Modal | `Dialog` (flat scrim, radius-xl card); title `Heading2` or `Title`, `Hint` body, actions right-aligned with the primary action last |
| Inline message | `Alert` |
| Icons | `Icon { source: Icons.<name>; }`. 24px grid, 1.75px outline, round caps. `ink-muted` at rest, `ink` on hover, `on-accent` on accent fills |
| Tooltips | `HoverTip` and `HoverDwell` (see `.agents/skills/angi-desktop-tooltips/SKILL.md`) |

- New icons follow the same grammar: a 24×24 viewBox, `stroke-width="1.75"`, round caps and joins,
  no fills. Add the SVG to `ui/icons/pebble/` and a line to `pebble/icons.slint`. Do not use Nerd
  Font glyphs for interface icons. File-type glyphs in Files are the one exception.
- Every interactive part forwards `pointer-moved(x, y)` from a `pointer-event` handler. On the device
  the desktop draws its own cursor from these coordinates, and `moved` alone fires only while a
  button is held.
- Keyboard: every control is reachable with Tab, activates with Return or Space, and shows the 2px
  focus ring only for keyboard focus (the components handle this).

### 6. Window and layout

- Apps live in a desktop tile. The tile chrome (header, title dot, Float, Full and Close) belongs to
  the desktop; do not draw your own title bar. Content starts below `Theme.header` and has the
  tile's `surface` behind it. The geometry is shared with Rust in `src/wm.rs` and `src/main.rs`;
  change it only in both places.
- Lay out as a bento: an outlined tile per topic, `space-4` to `space-6` gaps, at most one `accent`
  tile per screen. Inside a window, separate rows with a 1px `Divider` rather than boxing each one.
- A preference row is a `Label` with a `Hint` under it on the left and its control on the right
  (280px column), stacking the control below when the window is under 760px wide.
- Design for small tiles. Check 320, 620 and 1280px wide, and drop secondary content rather than
  clipping it.
- Long text wraps or elides; never let text overlap a control.

### 7. Both themes, every time

- Read every colour from `Theme`, so the light/dark switch is live; never cache a colour in Rust.
- Before you finish, look at your screen in both themes. On the workstation, run the preview with
  `XDG_CONFIG_HOME=<scratch dir>` and `theme=light` or `theme=dark` in
  `<scratch dir>/agx/appearance.conf`, or switch in Settings › Appearance.
- Colours that come from Rust (for example the storage heat scale in `storage/view.rs`) must read
  on both grounds. Pick values midway between the light and dark status tokens, and comment why.

### 8. Voice

Short, calm and literal. Labels are nouns ("Pointer speed"), buttons are verbs ("Restore defaults",
"Clear cache…"); use an ellipsis when the action asks for more input. Sentence case. Uppercase only
for small captions. No exclamation marks, no emoji. Anything a person sees says **ANGI** (see
`AGENTS.md`).

### 9. Accessibility

- Text meets AA in both themes when you use `ink` or `ink-muted` on any surface. Never put
  `ink-subtle` on anything except disabled text.
- Faint `line` strokes are below 3:1 by design. So every selected state changes fill, rings that
  carry meaning use `line-bold`, and focus is the 2px `focus` ring.
- Give icon-only controls an `accessible-label` (the `label` property on `IconButton`).

## Apps that are not Slint

A web app or other GUI that ANGI serves must look like the desktop:

1. Load `brand/pebble-line/tokens.css`, then `brand/pebble-line/web/bundle.css`, then React 18 and
   `bundle.js`. Use `window.PebbleLine` components (`Button`, `TextField`, `Card`, `Badge`, ...); the
   API is in `web/index.d.ts`.
2. Follow the system theme with `data-theme="light"` or `data-theme="dark"` on `<html>`. Read the
   person's choice from `~/.config/agx/appearance.conf` (`theme=dark` or `theme=light`) when the app
   runs on the device.
3. `bundle.css` imports Poppins from Google Fonts. The device may be offline, so prefer a local
   `@font-face` that points at `crates/agx-gui/ui/fonts/Poppins-*.ttf` when you package the app.
4. The same rules apply: tokens only, flat, outline or fill, one accent, status with words.

Terminal (TUI) apps keep the terminal's own palette. Use the ANSI colours sparingly, and keep status
in words as well as colour.

## Before you finish a screen

- [ ] No hex colours, std-widget controls, Nerd Font interface icons or font weight 700 in feature code.
- [ ] Only `Theme` tokens and `pebble.slint` components (or `tokens.css` and `PebbleLine` on the web).
- [ ] Checked in light and dark, and at small and large window sizes.
- [ ] Keyboard: Tab order, Return/Space and a visible focus ring.
- [ ] Status shown with words, not colour alone; icon-only controls labelled.
- [ ] Text follows the voice rules and says ANGI.
- [ ] The GUI tests pass (`scripts/cargo.sh test --manifest-path crates/agx-gui/Cargo.toml --locked`).
