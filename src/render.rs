//! Half-block rendering: two vertical pixels per terminal cell, so a 32x32
//! sprite occupies 32 columns and 16 rows.  Transparent pixels are left as the
//! terminal's own background -- nothing is painted behind the pet.

use crate::pet::{Pet, Rgb};
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor, queue, terminal};
use std::io::Write;

const UPPER: char = '▀';
const LOWER: char = '▄';

#[derive(Clone, Copy, PartialEq)]
pub enum Depth {
    True,
    Ansi256,
}

impl Depth {
    /// Truecolor unless the terminal says otherwise.
    pub fn detect() -> Depth {
        match std::env::var("COLORTERM").as_deref() {
            Ok("truecolor") | Ok("24bit") => Depth::True,
            _ => Depth::Ansi256,
        }
    }

    fn color(self, (r, g, b): Rgb) -> Color {
        match self {
            Depth::True => Color::Rgb { r, g, b },
            Depth::Ansi256 => Color::AnsiValue(ansi256(r, g, b)),
        }
    }
}

/// Nearest xterm-256 index: the 6x6x6 colour cube or the 24-step grey ramp.
fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    let cube = |v: u8| -> (u8, u8) {
        let levels = [0u8, 95, 135, 175, 215, 255];
        let mut best = 0;
        for (i, &l) in levels.iter().enumerate() {
            if v.abs_diff(l) < v.abs_diff(levels[best]) {
                best = i;
            }
        }
        (best as u8, levels[best])
    };
    let ((ri, rv), (gi, gv), (bi, bv)) = (cube(r), cube(g), cube(b));
    let cube_err = r.abs_diff(rv) as u32 + g.abs_diff(gv) as u32 + b.abs_diff(bv) as u32;

    let grey = (((r as u32 + g as u32 + b as u32) / 3).saturating_sub(8) / 10).min(23) as u8;
    let gv2 = 8 + grey * 10;
    let grey_err = r.abs_diff(gv2) as u32 + g.abs_diff(gv2) as u32 + b.abs_diff(gv2) as u32;

    if grey_err < cube_err {
        232 + grey
    } else {
        16 + 36 * ri + 6 * gi + bi
    }
}

/// A pixel buffer that is an even number of pixels tall, one cell = two pixels.
pub struct Screen {
    pub cols: u16,
    pub rows: u16,
    px: Vec<Option<Rgb>>,
    depth: Depth,
}

impl Screen {
    pub fn new(cols: u16, rows: u16, depth: Depth) -> Screen {
        Screen {
            cols,
            rows,
            px: vec![None; cols as usize * rows as usize * 2],
            depth,
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        self.px = vec![None; cols as usize * rows as usize * 2];
    }

    pub fn clear(&mut self) {
        self.px.iter_mut().for_each(|p| *p = None);
    }

    #[inline]
    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= self.cols as i32 || y >= self.rows as i32 * 2 {
            return;
        }
        let i = y as usize * self.cols as usize + x as usize;
        self.px[i] = Some(c);
    }

    /// Draw one sprite frame with its top-left pixel at (x, y).
    /// `mirror` flips it horizontally, which is how the pet faces left.
    pub fn blit(&mut self, pet: &Pet, frame: &[String], x: i32, y: i32, mirror: bool) {
        let w = pet.frame.width as i32;
        for (row, line) in frame.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let Some(rgb) = pet.color(ch) else { continue };
                let dx = if mirror { w - 1 - col as i32 } else { col as i32 };
                self.set(x + dx, y + row as i32, rgb);
            }
        }
    }

    /// Print the buffer as plain lines at the cursor, without moving it
    /// around -- used by `pet render` and by the tests.
    pub fn print(&self, out: &mut impl Write) -> std::io::Result<()> {
        for row in 0..self.rows {
            for col in 0..self.cols {
                let top = self.px[(row as usize * 2) * self.cols as usize + col as usize];
                let bot = self.px[(row as usize * 2 + 1) * self.cols as usize + col as usize];
                match (top, bot) {
                    (None, None) => queue!(out, ResetColor, Print(' '))?,
                    (Some(t), None) => queue!(
                        out,
                        SetBackgroundColor(Color::Reset),
                        SetForegroundColor(self.depth.color(t)),
                        Print(UPPER)
                    )?,
                    (None, Some(b)) => queue!(
                        out,
                        SetBackgroundColor(Color::Reset),
                        SetForegroundColor(self.depth.color(b)),
                        Print(LOWER)
                    )?,
                    (Some(t), Some(b)) => queue!(
                        out,
                        SetForegroundColor(self.depth.color(t)),
                        SetBackgroundColor(self.depth.color(b)),
                        Print(UPPER)
                    )?,
                }
            }
            queue!(out, ResetColor, Print('\n'))?;
        }
        out.flush()
    }

    /// Write the buffer out starting at terminal row `origin`.
    pub fn flush(&self, out: &mut impl Write, origin: u16) -> std::io::Result<()> {
        let mut fg: Option<Color> = None;
        let mut bg: Option<Color> = None;
        for row in 0..self.rows {
            queue!(out, cursor::MoveTo(0, origin + row))?;
            // Trailing empty cells need no output at all beyond the clear.
            queue!(out, terminal::Clear(terminal::ClearType::UntilNewLine))?;
            let (mut fg_now, mut bg_now) = (fg, bg);
            let mut line = String::with_capacity(self.cols as usize);
            let mut pending = 0u16;

            for col in 0..self.cols {
                let top = self.px[(row as usize * 2) * self.cols as usize + col as usize];
                let bot = self.px[(row as usize * 2 + 1) * self.cols as usize + col as usize];
                let (ch, f, b) = match (top, bot) {
                    (None, None) => {
                        pending += 1;
                        continue;
                    }
                    (Some(t), None) => (UPPER, Some(self.depth.color(t)), Some(Color::Reset)),
                    (None, Some(b)) => (LOWER, Some(self.depth.color(b)), Some(Color::Reset)),
                    (Some(t), Some(b)) => (
                        UPPER,
                        Some(self.depth.color(t)),
                        Some(self.depth.color(b)),
                    ),
                };
                if pending > 0 {
                    // Skip over transparent runs instead of painting them.
                    if !line.is_empty() {
                        queue!(out, Print(std::mem::take(&mut line)))?;
                    }
                    queue!(out, cursor::MoveTo(col, origin + row))?;
                    pending = 0;
                }
                if f != fg_now {
                    if !line.is_empty() {
                        queue!(out, Print(std::mem::take(&mut line)))?;
                    }
                    queue!(out, SetForegroundColor(f.unwrap()))?;
                    fg_now = f;
                }
                if b != bg_now {
                    if !line.is_empty() {
                        queue!(out, Print(std::mem::take(&mut line)))?;
                    }
                    queue!(out, SetBackgroundColor(b.unwrap()))?;
                    bg_now = b;
                }
                line.push(ch);
            }
            if !line.is_empty() {
                queue!(out, Print(line))?;
            }
            fg = fg_now;
            bg = bg_now;
        }
        queue!(out, ResetColor)?;
        out.flush()
    }
}
