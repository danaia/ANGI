# TextField

A pill outlined by a 1px `line` stroke on `surface`, with the label above it.

- Label in the `label` style (15/22, 500), always visible; the placeholder is an example, not the label.
- Optional leading `icon` or `prefix`. `type="password"` adds a show/hide toggle.
- Hover: stroke goes to `line-strong`. Focus: stroke and a second 1px ring in `ink` (2px total) — the one place the stroke gets heavy.
- `hint` sits under the field; `error` replaces it in `danger` and turns the stroke `danger`.

Accessibility note: the resting stroke is intentionally faint (about 1.3:1), below the 3:1 non-text guideline. Labels are always shown so a field never depends on its edge to be found.
