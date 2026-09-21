//! Terminal setup and teardown.  Whatever happens -- clean exit, panic, or a
//! Ctrl+C -- the terminal is put back the way we found it.

use crossterm::{cursor, execute, terminal};
use std::io::{stdout, Stdout, Write};

pub struct Term {
    pub out: Stdout,
    pub fullscreen: bool,
    /// First terminal row we are allowed to draw on.
    pub origin: u16,
    pub rows: u16,
}

impl Term {
    /// `rows` is only used for inline mode, where we reserve that many rows
    /// at the bottom of the current screen instead of taking it over.
    pub fn enter(fullscreen: bool, rows: u16) -> std::io::Result<Term> {
        let mut out = stdout();
        install_panic_hook();
        terminal::enable_raw_mode()?;

        let origin = if fullscreen {
            execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;
            0
        } else {
            execute!(out, cursor::Hide)?;
            // Scroll the reserved area into view, then find out where it began.
            write!(out, "{}", "\n".repeat(rows as usize))?;
            out.flush()?;
            // Not every terminal answers a cursor-position query; if this one
            // does not, assume we scrolled to the bottom of the screen.
            let bottom = terminal::size()?.1;
            cursor::position()
                .map(|(_, row)| row)
                .unwrap_or(bottom.saturating_sub(1))
                .saturating_sub(rows)
        };

        Ok(Term {
            out,
            fullscreen,
            origin,
            rows,
        })
    }

    pub fn size(&self) -> std::io::Result<(u16, u16)> {
        terminal::size()
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        if self.fullscreen {
            let _ = execute!(self.out, terminal::LeaveAlternateScreen, cursor::Show);
        } else {
            let _ = execute!(
                self.out,
                crossterm::style::ResetColor,
                cursor::MoveTo(0, self.origin),
                terminal::Clear(terminal::ClearType::FromCursorDown),
                cursor::Show
            );
        }
        let _ = self.out.flush();
    }
}

fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(stdout(), terminal::LeaveAlternateScreen, cursor::Show);
            previous(info);
        }));
    });
}
