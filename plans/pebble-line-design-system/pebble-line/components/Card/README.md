# Card

The building block of every layout. Four variants, no shadows:

- **outline** — `surface` + 1px `line`, `radius-lg`, `space-8` padding. Default.
- **hero** — the same with `radius-xl` and `space-10` padding. One per page (the login card, a feature card).
- **muted** — `surface-muted` fill, no stroke, `radius-md`. Nest inside an outline card for summaries ("Cart Summary").
- **accent** — solid `accent`, `on-accent` text. At most one per view.

Optional `eyebrow`, `title`, `actions`. Don't nest outline cards inside outline cards — use muted for the inner one so you never get double strokes.
