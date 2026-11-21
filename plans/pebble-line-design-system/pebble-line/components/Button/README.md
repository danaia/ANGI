# Button

A pill outlined by a 1px `line` stroke; emphasis comes from fill, never from depth.

- **secondary** (default) — `surface` fill, `line` stroke; hover darkens the stroke to `line-strong` and fills `surface-hover`. "Login", "Apply", "Checkout".
- **primary** — solid `accent` fill with `on-accent` text (near-black in light, near-white in dark). At most one per view.
- **ghost** — no stroke, `ink-muted` text, for Cancel and tertiary actions.
- **danger** — outlined, `danger` text; hover tints with `danger-soft`. Pair with a confirm dialog.

Sizes `sm` 36 / `md` 44 / `lg` 56px. Labels are verb-first, no trailing punctuation.

Props: `variant`, `size`, `icon`, `iconRight` (Icon names), `block`, plus any `<button>` attribute.
