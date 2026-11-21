# BentoGrid

A grid of outlined tiles in unequal spans. `BentoGrid` sets `columns` (default 4) and a minimum `rowHeight`; wrap each tile in `BentoCell` with `cols` / `rows`. Any Card or StatTile inside fills its cell.

Gap is `space-6` — with no shadows to clear, tiles sit closer than in Pebble. Mix one tall or wide tile per row, and fill at most one with `accent`.
