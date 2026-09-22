mod tab;

use std::cmp::{max, min};
use std::collections::BTreeMap;

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

use crate::tab::{more_message, tab_separator, tab_style};

// Powerline arrow glyph used as the tab separator (matches the stock tab-bar).
pub static ARROW_SEPARATOR: &str = "\u{E0B0}";

#[derive(Debug, Default, Clone)]
pub struct LinePart {
    part: String,
    len: usize,
    tab_index: Option<usize>,
}

// A clickable region on a rendered row: [col_start, col_end) maps to a 0-based tab position.
#[derive(Debug, Clone, Copy)]
struct ClickRegion {
    start: usize,
    end: usize,
    tab_position: usize,
}

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize, // 1-based, matching switch_tab_to's indexing
    mode_info: ModeInfo,
    // Per-row clickable regions, rebuilt every render so clicks map to the
    // tab currently under the cursor. Indexed by row (pane-relative line).
    click_map: Vec<Vec<ClickRegion>>,
    hide_session_name: bool,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.hide_session_name = configuration
            .get("hide_session_name")
            .map(|s| s == "true")
            .unwrap_or(false);
        // TabUpdate/ModeUpdate need read access; switch_tab_to needs change access.
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::PermissionRequestResult,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                if self.mode_info != mode_info {
                    should_render = true;
                }
                self.mode_info = mode_info;
            },
            Event::TabUpdate(tabs) => {
                if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
                    // tabs are indexed starting from 1 so we need to add 1
                    let active_tab_idx = active_tab_index + 1;
                    if self.active_tab_idx != active_tab_idx || self.tabs != tabs {
                        should_render = true;
                    }
                    self.active_tab_idx = active_tab_idx;
                    self.tabs = tabs;
                } else {
                    eprintln!("Could not find active tab.");
                }
            },
            Event::Mouse(me) => match me {
                Mouse::LeftClick(line, col) => {
                    if let Some(tab_position) = self.tab_at(line, col) {
                        let target = (tab_position + 1) as u32; // switch_tab_to is 1-based
                        if target as usize != self.active_tab_idx {
                            switch_tab_to(target);
                        }
                    }
                },
                Mouse::ScrollUp(_) => {
                    switch_tab_to(min(self.active_tab_idx + 1, self.tabs.len()) as u32);
                },
                Mouse::ScrollDown(_) => {
                    switch_tab_to(max(self.active_tab_idx.saturating_sub(1), 1) as u32);
                },
                _ => {},
            },
            Event::PermissionRequestResult(_) => {
                // Re-render once the user accepts so the bar appears immediately.
                should_render = true;
            },
            _ => {},
        }
        if self.tabs.is_empty() {
            // Avoid rendering before the first TabUpdate arrives.
            should_render = false;
        }
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.tabs.is_empty() || rows == 0 || cols == 0 {
            return;
        }
        let palette = self.mode_info.style.colors;
        let capabilities = self.mode_info.capabilities;
        let background = palette.text_unselected.background;

        // Tabs are shaded by the row they land on, so two stacked rows of ribbons
        // don't fuse into one block. Which row a tab lands on isn't known until
        // it's packed, and shading doesn't affect width, so: measure, assign
        // rows, then style for real.
        let measured: Vec<LinePart> = self
            .tabs
            .iter()
            .map(|t| tab_style(self.tab_name(t), t, false, palette, capabilities))
            .collect();

        let prefix = self.prefix(palette, cols);
        let prefix_width: usize = prefix.iter().map(|p| p.len).sum();
        let placement = assign_rows(&measured, prefix_width, cols, rows);
        // With arrow fonts the arrows separate tabs horizontally, so shading only
        // has to mark the row boundary. Without them nothing separates tabs, so
        // fall back to a checkerboard that alternates along the row as well.
        let arrow_fonts_available = !capabilities.arrow_fonts;

        let parts: Vec<LinePart> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let (row, col) = placement.get(i).copied().unwrap_or((0, 0));
                let shaded = if arrow_fonts_available {
                    row % 2 == 1
                } else {
                    (row + col) % 2 == 1
                };
                tab_style(self.tab_name(t), t, shaded, palette, capabilities)
            })
            .collect();

        let laid_out = self.layout_rows(parts, prefix, rows, cols, palette, capabilities);

        let bg = background_fill(&background);
        let mut output = String::new();
        for (i, row) in laid_out.iter().enumerate() {
            if i > 0 {
                output.push_str("\r\n");
            }
            for part in row {
                output.push_str(&part.part);
            }
            // Paint the remainder of the row with the bar background.
            output.push_str(&bg);
        }
        print!("{output}");
    }
}

impl State {
    // The label for a tab, including the placeholder shown while renaming.
    fn tab_name(&self, t: &TabInfo) -> String {
        let name = t.name.clone();
        if t.active && self.mode_info.mode == InputMode::RenameTab && name.is_empty() {
            String::from("Enter name...")
        } else {
            name
        }
    }

    // Optional leading "(session)" label on the first row. Returns empty when
    // hidden, unset, or it wouldn't fit.
    fn prefix(&self, palette: Styling, cols: usize) -> Vec<LinePart> {
        if self.hide_session_name {
            return vec![];
        }
        let session = self.mode_info.session_name.clone().unwrap_or_default();
        if session.is_empty() {
            return vec![];
        }
        let text = format!(" {session} ");
        let len = text.width();
        if len > cols {
            return vec![];
        }
        let fg = palette.text_unselected.base;
        let bg = palette.text_unselected.background;
        let part = style!(fg, bg).bold().paint(text).to_string();
        vec![LinePart {
            part,
            len,
            tab_index: None,
        }]
    }

    // Greedily pack tab ribbons into up to `max_rows` rows of width `cols`,
    // rebuilding the click map as it goes. If tabs exceed capacity, the last
    // row ends with a "+N …" overflow marker.
    fn layout_rows(
        &mut self,
        parts: Vec<LinePart>,
        prefix: Vec<LinePart>,
        max_rows: usize,
        cols: usize,
        palette: Styling,
        capabilities: PluginCapabilities,
    ) -> Vec<Vec<LinePart>> {
        let separator = tab_separator(capabilities);
        let total = parts.len();

        let mut rows: Vec<Vec<LinePart>> = Vec::new();
        let mut click_rows: Vec<Vec<ClickRegion>> = Vec::new();

        let mut cur: Vec<LinePart> = Vec::new();
        let mut cur_click: Vec<ClickRegion> = Vec::new();
        let mut cur_width = 0usize;

        // The prefix lives on the first row and is not clickable.
        for p in prefix {
            cur_width += p.len;
            cur.push(p);
        }

        let mut idx = 0usize;
        while idx < total {
            let part = parts[idx].clone();
            let fits = cur_width + part.len <= cols;
            let row_empty = cur.is_empty();

            if !fits && !row_empty {
                // Current row is full. Either wrap to a new row, or — if we've
                // exhausted our row budget — emit the overflow marker and stop.
                let is_last_row = rows.len() + 1 >= max_rows;
                if is_last_row {
                    let remaining = total - idx;
                    let more = more_message(remaining, palette, separator, idx);
                    // Make room for the marker by dropping trailing tabs if needed.
                    while cur_width + more.len > cols && !cur.is_empty() {
                        if let Some(removed) = cur.pop() {
                            cur_width -= removed.len;
                            cur_click.pop();
                        }
                    }
                    let start = cur_width;
                    cur_width += more.len;
                    cur_click.push(ClickRegion {
                        start,
                        end: cur_width,
                        tab_position: idx, // jump to first hidden tab
                    });
                    cur.push(more);
                    break;
                }
                rows.push(std::mem::take(&mut cur));
                click_rows.push(std::mem::take(&mut cur_click));
                cur_width = 0;
            }

            let start = cur_width;
            cur_width += part.len;
            if let Some(pos) = part.tab_index {
                cur_click.push(ClickRegion {
                    start,
                    end: cur_width,
                    tab_position: pos,
                });
            }
            cur.push(part);
            idx += 1;
        }

        if !cur.is_empty() {
            rows.push(cur);
            click_rows.push(cur_click);
        }

        self.click_map = click_rows;
        rows
    }

    // Map a pane-relative mouse click to a 0-based tab position, if any.
    fn tab_at(&self, line: isize, col: usize) -> Option<usize> {
        if line < 0 {
            return None;
        }
        let row = self.click_map.get(line as usize)?;
        for region in row {
            if col >= region.start && col < region.end {
                return Some(region.tab_position);
            }
        }
        None
    }
}

// Where each tab lands: (row, position within that row), mirroring the wrap
// rule in `layout_rows`. Tabs past the row budget all report the last row;
// they're hidden behind the overflow marker, so their shading is never seen.
fn assign_rows(
    parts: &[LinePart],
    prefix_width: usize,
    cols: usize,
    max_rows: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(parts.len());
    let mut row = 0usize;
    let mut col = 0usize;
    let mut cur_width = prefix_width;
    for part in parts {
        if cur_width + part.len > cols && cur_width > 0 && row + 1 < max_rows {
            row += 1;
            col = 0;
            cur_width = 0;
        }
        out.push((row, col));
        col += 1;
        cur_width += part.len;
    }
    out
}

fn background_fill(color: &PaletteColor) -> String {
    match color {
        PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{r};{g};{b}m\u{1b}[0K"),
        PaletteColor::EightBit(c) => format!("\u{1b}[48;5;{c}m\u{1b}[0K"),
    }
}
