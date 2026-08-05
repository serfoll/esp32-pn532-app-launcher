# Store Badge on Gallery Cards

## Problem Statement

How might we show, at a glance on each gallery card, which storefront a
game was installed through — like a CD-case logo in the corner — without
building out full multi-store support before it's needed?

## Recommended Direction

Detect the store at scan time (reusing `find_steam_app_id` from
`src-tauri/src/launch/mod.rs`), persist it on the `Game` record, and render
a small icon badge in the corner of the card in `renderGallery`
(`src/gallery/gallery.ts`). A new `Settings.showStoreBadges: boolean`
(default `true`) gates rendering, following the same pattern as the
existing `showOutputLog` toggle.

Scope for v1 is Steam only — it's the only store this codebase already
detects. But the user wants the shape to survive adding a second store
without a rewrite, so the detection code should be a small, generic
surface (a `Store` enum + one `detect_store()` entry point that tries each
known detector) rather than a Steam-specific function called directly from
`scan`. Concretely:

```rust
pub enum Store {
    Steam,
}

pub fn detect_store(folder_path: &Path) -> Option<Store> {
    find_steam_app_id(folder_path).map(|_| Store::Steam)
    // future: .or_else(|| find_gog_id(folder_path).map(|_| Store::Gog))
}
```

This is deliberately *not* a trait/plugin system — one implementor doesn't
justify an interface. It's just an enum and a dispatch function shaped so
"add GOG" is one new match arm plus one new detector function, not a
redesign.

## Key Assumptions to Validate

- [ ] A corner badge reads as useful rather than clutter at the gallery's
      actual card size — check visually once built, adjust size/position if
      it competes with artwork.
- [ ] Existing catalog entries lacking `store` (added before this ships)
      is an acceptable gap, closed by the next rescan — same as artwork
      backfill today.
- [ ] Steam-only coverage doesn't feel incomplete to the user day-to-day —
      revisit if most of their library turns out to be non-Steam.

## MVP Scope

- `Store` enum (`Steam` variant only) + `detect_store()` dispatcher in
  `src-tauri/src/launch/mod.rs` or a new small module, wrapping the
  existing `find_steam_app_id`.
- `Game.store: Option<Store>` field, populated during folder scan.
- `Settings.showStoreBadges: bool` (default `true`), rendered as a toggle
  in `src/settings/settings.ts` alongside existing boolean settings.
- Badge rendering in `renderGallery` (`src/gallery/gallery.ts`): small
  Steam icon (bundled SVG under `src/assets/`) positioned top-left/top of
  the card, shown only when `game.store` is set and the setting is on.
- No badge at all for games with no detected store.

## Not Doing (and Why)

- **GOG/Epic/EA/Ubisoft detectors** — no code touches this repo's install
  folders for those stores yet; adding them is real per-store work
  (Epic/EA/Ubisoft need centralized manifest lookups, not a per-folder
  file check like Steam/GOG). Deferred until Steam-only proves the badge
  is worth having.
- **Generic "unknown store" badge** — user chose no-badge-when-unknown;
  don't build a placeholder icon nobody asked for.
- **Store-based actions** (click badge to open store page, filter/sort by
  store) — out of scope, this is a visual indicator only.
- **Runtime icon fetching from store CDNs** — bundled static SVG is
  simpler and has no network failure mode; revisit only if icon licensing
  or freshness becomes an actual problem.
- **Auto-rescan on upgrade to backfill `store` on old entries** — existing
  "Refresh artwork" / rescan flow already covers this; no new migration
  code needed.

## Open Questions

- Exact badge visual (icon-only vs. small pill with store name) — decide
  during implementation by eyeballing it against real card art, not worth
  a spec debate.
