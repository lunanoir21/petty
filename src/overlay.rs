//! Overlay mode: the pet is composited over the terminal's text by the
//! terminal itself, so it reserves no rows and leaves no scrollback.
//!
//! It runs detached from the shell, which means it must never touch stdin --
//! the shell owns that.  Everything the pet needs to hear arrives through a
//! small control file instead, which is also the hook for shell events
//! (`pet event grumpy` after a failed command, say), or through the mouse.

use crate::app::Player;
use crate::behavior::{Brain, Event, State};
use crate::kitty::Kitty;
use crate::mouse::Mouse;
use crate::pet::Pet;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Extras the pet may drop into on its own, in the order they were authored.
const EXTRAS: &[&str] = &[
    "stretch", "curious", "look_left", "look_right", "back_view", "grumpy", "jump",
];

/// Where the running pet keeps its pid and its inbox.
pub fn runtime_dir() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("pet")
}

pub fn pid_file() -> PathBuf {
    runtime_dir().join("pet.pid")
}

pub fn event_file() -> PathBuf {
    runtime_dir().join("event")
}

/// Post an event to a running pet.  Returns false when none is running.
pub fn send(event: &str) -> io::Result<bool> {
    if !pid_file().exists() {
        return Ok(false);
    }
    fs::create_dir_all(runtime_dir())?;
    fs::write(event_file(), event)?;
    Ok(true)
}

fn take_event() -> Option<Event> {
    let text = fs::read_to_string(event_file()).ok()?;
    let _ = fs::remove_file(event_file());
    Event::parse(&text)
}

pub struct Options {
    /// device pixels per sprite pixel, or None to size the pet to the window
    pub scale: Option<u32>,
    /// rows to leave clear below the pet, so it can sit above the prompt
    pub lift: u16,
    /// walking speed in device pixels per second
    pub speed: f32,
}

/// The pet should be a readable size without swamping the window: about a
/// sixth of the window height, and never smaller than 2x.
fn auto_scale(window_px: u32, sprite_px: u32) -> u32 {
    (window_px / 6 / sprite_px.max(1)).clamp(2, 12)
}

pub fn run(pet: &Pet, opts: Options) -> io::Result<()> {
    if !Kitty::available() {
        return Err(io::Error::other(
            "overlay mode needs the kitty graphics protocol (this is not a kitty terminal)",
        ));
    }

    // Write to the controlling terminal, not to stdout: stdout is a log file
    // once the pet has been started in the background.
    let mut tty = fs::OpenOptions::new().write(true).open("/dev/tty")?;

    let (cols, rows) = crossterm::terminal::size()?;
    let mut kitty = Kitty::new(1);
    let chosen = |rows: u16, kitty: &Kitty| {
        opts.scale
            .unwrap_or_else(|| auto_scale(rows as u32 * kitty.cell_h, pet.frame.height as u32))
    };
    kitty.set_scale(chosen(rows, &kitty));

    let extras: Vec<(String, Duration)> = EXTRAS
        .iter()
        .filter(|name| pet.has(name))
        .map(|name| {
            let anim = pet.animation_or_sit(name);
            let span = anim.frames.len() as f32 / anim.fps;
            (name.to_string(), Duration::from_secs_f32(span.max(0.4)))
        })
        .collect();

    let mut brain = Brain::new(
        (cols as u32 * kitty.cell_w) as f32,
        floor(rows, &kitty, pet, opts.lift),
        sprite_size(&kitty, pet),
        opts.speed,
        extras,
    );
    let mut player = Player::new(pet, brain.animation());

    // No mouse, or not Hyprland: the pet simply cannot be picked up.
    let mouse = Mouse::new();
    if mouse.is_none() {
        eprintln!("pet: no readable mouse found, so the pet cannot be picked up");
    }
    let mut was_down = false;
    let mut grab_offset = (0.0f32, 0.0f32);
    let mut geometry: Option<(i32, i32, i32, i32)> = None;
    let mut geometry_checked = Instant::now() - Duration::from_secs(9);

    fs::create_dir_all(runtime_dir())?;
    fs::write(pid_file(), std::process::id().to_string())?;
    let _ = fs::remove_file(event_file());

    let mut last = Instant::now();
    let mut layout = (cols, rows);
    let result = loop {
        if let Some(event) = take_event() {
            if event == Event::Quit {
                break Ok(());
            }
            brain.feed(event);
        }

        let (cols, rows) = crossterm::terminal::size()?;
        if (cols, rows) != layout {
            layout = (cols, rows);
            // The pet grows and shrinks with the window it lives in.
            let want = chosen(rows, &kitty);
            if want != kitty.scale {
                kitty.clear(&mut tty)?;
                kitty.set_scale(want);
                brain.sprite_w = sprite_size(&kitty, pet).0;
                brain.sprite_h = sprite_size(&kitty, pet).1;
            }
            brain.resize(
                (cols as u32 * kitty.cell_w) as f32,
                floor(rows, &kitty, pet, opts.lift),
            );
            geometry = None;
        }

        // --- picking the pet up ------------------------------------------
        if let Some(mouse) = &mouse {
            // The window can be moved or resized under us; re-ask now and then.
            if geometry.is_none() || geometry_checked.elapsed() > Duration::from_secs(2) {
                geometry = mouse.window_rect();
                geometry_checked = Instant::now();
            }
            let down = mouse.right_down();
            if let (Some(rect), Some(pointer)) = (geometry, down.then(|| mouse.cursor()).flatten())
            {
                let (px, py) = to_window(rect, pointer, cols, rows, &kitty);
                if !was_down {
                    let (bx, by, bw, bh) = brain.bounds();
                    if px >= bx && px <= bx + bw && py >= by && py <= by + bh {
                        grab_offset = (px - bx, py - by);
                        brain.grab();
                    }
                } else if brain.grabbed() {
                    brain.drag_to(px - grab_offset.0, py - grab_offset.1);
                }
            }
            if was_down && !down && brain.grabbed() {
                brain.release();
            }
            was_down = down;
        }

        let now = Instant::now();
        brain.tick(now - last);
        last = now;

        player.play(brain.animation());
        player.tick();

        let col = (brain.x as u32 / kitty.cell_w) as u16;
        let off_x = brain.x as u32 % kitty.cell_w;
        let row = (brain.y.max(0.0) as u32 / kitty.cell_h) as u16;
        let off_y = brain.y.max(0.0) as u32 % kitty.cell_h;

        if let Err(e) = kitty.show(
            &mut tty,
            pet,
            brain.animation(),
            player.index(),
            col,
            row.min(rows.saturating_sub(1)),
            off_x,
            off_y,
            brain.facing_left && pet.animation_or_sit(brain.animation()).mirrors(),
        ) {
            // The terminal went away; nothing left to draw on.
            break Err(e);
        }

        // Being carried or falling wants every frame; sleeping does not.
        let pace = match brain.state {
            State::Carried | State::Falling => 16,
            State::Sleep => 80,
            _ => 40,
        };
        std::thread::sleep(Duration::from_millis(pace));
    };

    let _ = kitty.clear(&mut tty);
    let _ = tty.flush();
    let _ = fs::remove_file(pid_file());
    result
}

fn sprite_size(kitty: &Kitty, pet: &Pet) -> (f32, f32) {
    let (w, h) = kitty.sprite_px(pet);
    (w as f32, h as f32)
}

/// Top edge of the pet when it is standing on the floor of the window.
fn floor(rows: u16, kitty: &Kitty, pet: &Pet, lift: u16) -> f32 {
    let bottom = rows.saturating_sub(lift) as u32 * kitty.cell_h;
    (bottom.saturating_sub(kitty.sprite_px(pet).1)) as f32
}

/// Compositor coordinates -> pixels inside the terminal's text area.  The grid
/// is centred in the window, so the leftover pixels are split between the
/// edges; close enough to grab a cat with.
fn to_window(
    rect: (i32, i32, i32, i32),
    pointer: (i32, i32),
    cols: u16,
    rows: u16,
    kitty: &Kitty,
) -> (f32, f32) {
    let (wx, wy, ww, wh) = rect;
    let grid_w = cols as i32 * kitty.cell_w as i32;
    let grid_h = rows as i32 * kitty.cell_h as i32;
    let pad_x = (ww - grid_w).max(0) / 2;
    let pad_y = (wh - grid_h).max(0) / 2;
    (
        (pointer.0 - wx - pad_x) as f32,
        (pointer.1 - wy - pad_y) as f32,
    )
}
