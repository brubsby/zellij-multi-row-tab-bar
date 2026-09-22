# multi-row-tab-bar

A [zellij](https://zellij.dev) tab-bar plugin that **wraps tabs across multiple rows** so
you can see all of them at once, instead of the stock single-line bar that collapses
overflow into a `+N` indicator.

Forks the per-tab styling from zellij's built-in `tab-bar` (so individual tabs look
identical), but replaces the single-line layout with a multi-row packer.

## Features

- **Multi-row layout** — tabs flow left-to-right and wrap onto the next row.
- **Click-to-switch** — click any tab (on any row) to focus it.
- **Scroll-to-switch** — scroll over the bar to move to the next/previous tab.
- **Overflow marker** — if tabs exceed the rows available, the last row ends with
  `+N …`; click it to jump to the first hidden tab.
- **Theme-aware** — uses your zellij theme's ribbon/text colors.
- Active-tab highlight, alternate-tab shading, fullscreen/sync/bell indicators.

## Build

```bash
rustup target add wasm32-wasip1            # one-time
cargo build --release --target wasm32-wasip1
# -> target/wasm32-wasip1/release/multi-row-tab-bar.wasm
```

`./install.sh` builds and copies the wasm into `~/.config/zellij/plugins/`.

## Use

A layout is installed at `~/.config/zellij/layouts/multi-row.kdl`. Launch with it:

```bash
zellij --layout multi-row
```

Or make it the default in `~/.config/zellij/config.kdl`:

```kdl
default_layout "multi-row"
```

### Granting permissions

The plugin needs `ReadApplicationState` + `ChangeApplicationState`. zellij normally
prompts for these on first launch, but **that prompt cannot be answered for this
plugin**: the bar calls `set_selectable(false)`, and zellij only routes the prompt's
`y`/`n` to the *focused* pane — a non-selectable pane can never be focused. The bar
will sit on `Allow? (y/n)` forever.

Grant them up front instead, by creating `~/.cache/zellij/permissions.kdl`:

```kdl
"/home/<you>/.config/zellij/plugins/multi-row-tab-bar.wasm" {
    ReadApplicationState
    ChangeApplicationState
}
```

Note the key is the **bare path** — no `file:` prefix, even though the layout
reference and the plugin cache directory both use one.

### Configuring the number of rows

The row budget is the **height of the bar's pane** in the layout. Edit
`size=2` in `multi-row.kdl` to allow more rows:

```kdl
pane size=3 borderless=true {
    plugin location="file:/home/<you>/.config/zellij/plugins/multi-row-tab-bar.wasm"
}
```

Plugin config options (inside the `plugin { }` block):

- `hide_session_name true|false` — drop the leading `(session)` label to reclaim space.

## Dev loop

With the `develop-rust-plugin` harness or manually: rebuild and reload.

```bash
cargo build --release --target wasm32-wasip1 \
  && cp target/wasm32-wasip1/release/multi-row-tab-bar.wasm ~/.config/zellij/plugins/
# then in zellij: Ctrl o, p (plugin manager) to reload, or restart the session
```

## Current limitation: fixed row budget

You reserve a fixed number of rows (`size=N`) and the plugin packs tabs into up to
`N` rows, rendering only the rows it needs and showing a `+N …` overflow marker when
tabs exceed `N`. Set `N` to the most rows you ever want.

True auto-height — the bar growing and shrinking the space it reserves as tabs come
and go — **is not possible** on zellij 0.44/0.45. This was measured, not assumed; see
the `auto-height` branch for the spike.

`zellij-tile` does expose `resize_pane_with_id(ResizeStrategy, PaneId)` (since 0.44),
and a plugin can address its own pane via `PaneId::Plugin(get_plugin_ids().plugin_id)`.
Two things block it:

1. **A pane pinned with `size=N` in the layout ignores the resize entirely.** The
   command is accepted and silently dropped — `resize_pane_with_id` fires a
   `ScreenInstruction` whose result is discarded, so the plugin gets no error. In
   testing, 28 resize commands left the height at 2.
2. **Unpin it and you hit a floor of 5 rows.** Without `size=N` the resize *is*
   honored — the bar walked 19 → 17 → 15 → 13 → 11 → 10 → 8 → 6 rows — but
   `can_reduce_pane_height` refuses any step that would leave a pane below
   `MIN_TERMINAL_HEIGHT` (5), and steps are `RESIZE_PERCENT` (5% of the viewport),
   not single rows.

A tab bar wants 1–3 rows. Pinned, it can't move; unpinned, it can't get under ~6.
Auto-height needs either a resize-to-exact-size plugin command or an exemption from
`MIN_TERMINAL_HEIGHT` for non-selectable panes.
