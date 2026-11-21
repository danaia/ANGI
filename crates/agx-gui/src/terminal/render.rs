//! Styled row runs; only changed rows reach Slint and its existing glyph cache.
use super::session::Proxy;
use crate::{TerminalRow, TerminalRun};
use alacritty_terminal::{
    Term,
    grid::Dimensions,
    term::{cell::Flags, color::Colors},
    vte::ansi::{Color, Rgb},
};
use slint::{ModelRc, VecModel};
const ANSI: [u32; 16] = [
    0x151820, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xc0caf5, 0x565f89,
    0xff9eae, 0xb9f27c, 0xffd98e, 0x9dbaff, 0xd5b5ff, 0xa4e6ff, 0xffffff,
];
pub(super) fn palette(index: usize) -> Rgb {
    let n = match index {
        0..=15 => ANSI[index],
        16..=231 => {
            let n = index - 16;
            let v = |c| if c == 0 { 0 } else { 55 + 40 * c };
            (v(n / 36) << 16 | v(n / 6 % 6) << 8 | v(n % 6)) as u32
        }
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u32;
            v << 16 | v << 8 | v
        }
        // Pebble Line dark ground and ink. The terminal stays dark in both desktop themes because
        // the ANSI palette is tuned for it; ui/terminal.slint uses the same ground (Theme.terminal-ground).
        257 | 268 => 0x141414,
        258 => 0xececea,
        259..=266 => {
            let v = ANSI[index - 259];
            ((v & 0xfefefe) >> 1) as u32
        }
        _ => 0xececea,
    };
    Rgb {
        r: (n >> 16) as u8,
        g: (n >> 8) as u8,
        b: n as u8,
    }
}
fn resolve(color: Color, colors: &Colors, flags: Flags, foreground: bool) -> slint::Color {
    let index = match color {
        Color::Named(n) => Some(n as usize),
        Color::Indexed(n) => Some(n as usize),
        Color::Spec(_) => None,
    };
    let index = index.map(|n| {
        if foreground && flags.contains(Flags::BOLD) && n < 8 {
            n + 8
        } else {
            n
        }
    });
    let rgb = match color {
        Color::Spec(rgb) => rgb,
        _ => {
            let i = index.unwrap();
            colors[i].unwrap_or_else(|| palette(i))
        }
    };
    let dim = |v: u8| {
        if foreground && flags.contains(Flags::DIM) {
            (v as u16 * 2 / 3) as u8
        } else {
            v
        }
    };
    slint::Color::from_rgb_u8(dim(rgb.r), dim(rgb.g), dim(rgb.b))
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Run {
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub fg: slint::Color,
    pub bg: slint::Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}
pub(super) fn rows(term: &Term<Proxy>) -> Vec<Vec<Run>> {
    let content = term.renderable_content();
    let mut rows = vec![Vec::<Run>::new(); term.screen_lines()];
    for item in content.display_iter {
        let y = (item.point.line.0 + content.display_offset as i32) as usize;
        if y >= rows.len() {
            continue;
        }
        let cell = item.cell;
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let mut fg = resolve(cell.fg, content.colors, cell.flags, true);
        let mut bg = resolve(cell.bg, content.colors, cell.flags, false);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if content
            .selection
            .as_ref()
            .is_some_and(|selection| selection.contains(item.point))
        {
            // Pebble dark line-strong: a neutral selection that keeps ANSI text legible.
            bg = slint::Color::from_rgb_u8(69, 69, 69);
        }
        let mut text = if cell.flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            cell.c
        }
        .to_string();
        if let Some(extra) = cell.zerowidth() {
            text.extend(extra);
        }
        let run = Run {
            col: item.point.column.0,
            cells: if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            },
            text,
            fg,
            bg,
            bold: cell.flags.contains(Flags::BOLD),
            italic: cell.flags.contains(Flags::ITALIC),
            underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
            strike: cell.flags.contains(Flags::STRIKEOUT),
        };
        if let Some(last) = rows[y].last_mut().filter(|last| {
            last.col + last.cells == run.col
                && last.fg == run.fg
                && last.bg == run.bg
                && last.bold == run.bold
                && last.italic == run.italic
                && last.underline == run.underline
                && last.strike == run.strike
        }) {
            last.text.push_str(&run.text);
            last.cells += run.cells;
        } else {
            rows[y].push(run);
        }
    }
    rows
}
pub(super) fn model(runs: &[Run]) -> TerminalRow {
    TerminalRow {
        runs: ModelRc::new(VecModel::from(
            runs.iter()
                .map(|r| TerminalRun {
                    col: r.col as i32,
                    cells: r.cells as i32,
                    text: r.text.clone().into(),
                    fg: r.fg,
                    bg: r.bg,
                    bold: r.bold,
                    italic: r.italic,
                    underline: r.underline,
                    strike: r.strike,
                })
                .collect::<Vec<_>>(),
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xterm_cube_and_grayscale() {
        assert_eq!(palette(16), Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(
            palette(231),
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            palette(255),
            Rgb {
                r: 238,
                g: 238,
                b: 238
            }
        );
    }
}
