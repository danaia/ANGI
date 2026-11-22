//! Keyboard handling for a browser: navigation, selection, file shortcuts and type-ahead.
use super::hub::{Effect, Hub, PICKER};
use std::time::{Duration, Instant};

/// Typed letters keep extending the prefix for this long after the last one.
const TYPE_AHEAD: Duration = Duration::from_millis(900);

impl Hub {
    /// `None` means the key was not for the browser and should bubble to the desktop.
    /// Slint names the special keys ("up", "enter", "f2", …); letters arrive as typed.
    pub fn key(&mut self, pane: usize, key: &str, ctrl: bool, alt: bool, shift: bool, page: usize) -> Option<Effect> {
        self.panes[pane].page = page.max(1);
        let len = self.panes[pane].model.len();
        let cursor = self.panes[pane].model.cursor();

        if alt && !ctrl {
            match key {
                "left" => self.back(pane),
                "right" => self.forward(pane),
                "up" => self.up(pane),
                "home" => drop(self.command(pane, "home", "")),
                _ => return None,
            }
            return Some(Effect::None);
        }
        if ctrl {
            let letter = match key.chars().next() {
                Some(c) if key.chars().count() == 1 && (1..=26).contains(&(c as u32)) => ((c as u8) + 96) as char,
                Some(c) if key.chars().count() == 1 => c.to_ascii_lowercase(),
                _ => '\0',
            };
            let command = match (letter, shift) {
                ('a', _) => "select-all",
                ('c', _) => "copy",
                ('x', _) => "cut",
                ('v', _) => "paste",
                ('z', false) => "undo",
                ('z', true) | ('y', _) => "redo",
                ('h', _) => "hidden",
                ('n', true) => "new-folder",
                ('r', _) => "refresh",
                _ => "",
            };
            if !command.is_empty() {
                return Some(self.command(pane, command, ""));
            }
            // Ctrl+arrows move the cursor, Ctrl+Space toggles the row under it.
            match (key, cursor) {
                ("up" | "down", Some(row)) => {
                    let next = if key == "up" { row.saturating_sub(1) } else { row + 1 };
                    self.panes[pane].model.focus_row(next);
                    self.panes[pane].cursor_serial += 1;
                }
                (" ", Some(_)) => self.panes[pane].model.toggle_cursor(),
                _ => return None,
            }
            return Some(Effect::None);
        }

        let step = |delta: isize| cursor.map_or(if delta < 0 { len.saturating_sub(1) } else { 0 }, |r| r.saturating_add_signed(delta));
        let target = match key {
            "up" => Some(step(-1)),
            "down" => Some(step(1)),
            "pageup" => Some(step(-(page as isize))),
            "pagedown" => Some(step(page as isize)),
            "home" => Some(0),
            "end" => Some(len.saturating_sub(1)),
            _ => None,
        };
        if let Some(row) = target {
            if len > 0 {
                self.panes[pane].model.move_cursor(row, shift);
                self.panes[pane].cursor_serial += 1;
            }
            return Some(Effect::None);
        }
        // A pending permanent-delete question is answered from the keyboard too.
        if let Some(super::pane::Prompt::Delete(_)) = self.panes[pane].prompt {
            match key {
                "enter" => drop(self.command(pane, "prompt-accept", "")),
                "escape" => drop(self.command(pane, "prompt-cancel", "")),
                _ => {}
            }
            return Some(Effect::None);
        }
        match key {
            "enter" => {
                let selected = self.panes[pane].model.selected();
                if pane == PICKER && selected.len() != 1 {
                    return Some(self.command(pane, "choose", ""));
                }
                match cursor {
                    Some(row) if selected.len() <= 1 => return Some(self.activate(pane, row)),
                    _ => return None,
                }
            }
            "backspace" => self.up(pane),
            "delete" => {
                let command = if shift { "delete" } else { "trash" };
                drop(self.command(pane, command, ""));
            }
            "f2" => drop(self.command(pane, "rename", "")),
            "f5" => self.reload(pane),
            "escape" => {
                let p = &mut self.panes[pane];
                if p.search.is_some() {
                    drop(self.command(pane, "clear-search", ""));
                } else if p.model.stats().selected > 0 {
                    p.model.clear();
                } else {
                    return None;
                }
            }
            text if !alt && text.chars().count() == 1 && !text.chars().next().unwrap().is_control() => {
                self.type_ahead(pane, text);
            }
            _ => return None,
        }
        Some(Effect::None)
    }

    /// Jumps to the next row whose name starts with what was just typed.
    fn type_ahead(&mut self, pane: usize, text: &str) {
        let p = &mut self.panes[pane];
        let now = Instant::now();
        let fresh = p.typed.1.is_none_or(|last| now.duration_since(last) > TYPE_AHEAD);
        if fresh {
            p.typed.0.clear();
        }
        p.typed.0.push_str(text);
        p.typed.1 = Some(now);
        // A repeated single letter cycles through names, as in other file managers.
        let repeated = p.typed.0.chars().count() > 1 && p.typed.0.chars().all(|c| c == p.typed.0.chars().next().unwrap());
        let (prefix, from) = if repeated {
            (p.typed.0.chars().next().unwrap().to_string(), p.model.cursor().map_or(0, |c| c + 1))
        } else {
            (p.typed.0.clone(), p.model.cursor().unwrap_or(0))
        };
        if let Some(row) = p.model.find_prefix(&prefix, from) {
            p.model.move_cursor(row, false);
            p.cursor_serial += 1;
        }
    }
}
