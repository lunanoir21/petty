//! Kitty graphics protocol backend.
//!
//! The pet is transmitted as an RGBA image and shown with a *placement* at an
//! absolute screen cell.  Nothing is written into the text grid, so the pet
//! costs no terminal rows and no scrollback: it is composited over the text by
//! the terminal itself (z >= 0 draws above the text).
//!
//! Frames are pre-scaled here with nearest-neighbour sampling and placed at
//! their natural pixel size -- asking the terminal to scale would blur the
//! pixel art.

use crate::pet::Pet;
use std::collections::HashMap;
use std::io::{self, Write};

const ESC: &str = "\x1b";
/// Base64 payload per escape sequence, as required by the protocol.
const CHUNK: usize = 4096;

pub struct Kitty {
    base_id: u32,
    next_slot: u32,
    /// (animation, frame, mirrored) -> transmitted image id
    sent: HashMap<(String, usize, bool), u32>,
    /// the placement currently on screen
    shown: Option<(u32, u32)>,
    placement: u32,
    pub cell_w: u32,
    pub cell_h: u32,
    pub scale: u32,
}

impl Kitty {
    /// True when this terminal speaks the graphics protocol.
    pub fn available() -> bool {
        std::env::var_os("KITTY_WINDOW_ID").is_some()
            || std::env::var("TERM").is_ok_and(|t| t.contains("kitty"))
    }

    /// `scale` is how many device pixels one sprite pixel becomes.
    pub fn new(scale: u32) -> Kitty {
        let (cell_w, cell_h) = cell_size();
        Kitty {
            // Keep our image ids well away from whatever else is on this
            // screen; two pets in one terminal get different ranges too.
            base_id: 0x0CA7_0000 + (std::process::id() % 0xFF) * 0x100,
            next_slot: 0,
            sent: HashMap::new(),
            shown: None,
            placement: 1,
            cell_w,
            cell_h,
            scale: scale.max(1),
        }
    }

    /// Change how big the pet is drawn.  Every transmitted frame is at the
    /// old size, so the caller must `clear` first.
    pub fn set_scale(&mut self, scale: u32) {
        self.scale = scale.max(1);
        self.sent.clear();
        self.shown = None;
    }

    /// Size of the pet on screen, in device pixels.
    pub fn sprite_px(&self, pet: &Pet) -> (u32, u32) {
        (
            pet.frame.width as u32 * self.scale,
            pet.frame.height as u32 * self.scale,
        )
    }

    /// Transmit a frame if it has not been sent yet, and return its image id.
    fn ensure(
        &mut self,
        out: &mut impl Write,
        pet: &Pet,
        anim: &str,
        idx: usize,
        mirror: bool,
    ) -> io::Result<u32> {
        let key = (anim.to_string(), idx, mirror);
        if let Some(&id) = self.sent.get(&key) {
            return Ok(id);
        }
        let id = self.base_id + self.next_slot;
        self.next_slot += 1;

        let frame = &pet.animation_or_sit(anim).frames[idx];
        let (w, h) = self.sprite_px(pet);
        let rgba = self.rasterise(pet, frame, w, h, mirror);
        let payload = base64(&rgba);

        let mut chunks = payload.as_bytes().chunks(CHUNK).peekable();
        let mut first = true;
        while let Some(chunk) = chunks.next() {
            let more = if chunks.peek().is_some() { 1 } else { 0 };
            if first {
                write!(
                    out,
                    "{ESC}_Ga=t,f=32,s={w},v={h},i={id},q=2,m={more};"
                )?;
                first = false;
            } else {
                write!(out, "{ESC}_Gm={more};")?;
            }
            out.write_all(chunk)?;
            write!(out, "{ESC}\\")?;
        }
        self.sent.insert(key, id);
        Ok(id)
    }

    /// Nearest-neighbour upscale straight into RGBA; transparent stays alpha 0.
    fn rasterise(&self, pet: &Pet, frame: &[String], w: u32, h: u32, mirror: bool) -> Vec<u8> {
        let (sw, sh) = (pet.frame.width as u32, pet.frame.height as u32);
        let rows: Vec<Vec<char>> = frame.iter().map(|r| r.chars().collect()).collect();
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            let sy = (y * sh / h) as usize;
            for x in 0..w {
                let px = if mirror { w - 1 - x } else { x };
                let sx = (px * sw / w) as usize;
                let Some((r, g, b)) = rows
                    .get(sy)
                    .and_then(|row| row.get(sx))
                    .and_then(|&c| pet.color(c))
                else {
                    continue;
                };
                let i = ((y * w + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        out
    }

    /// Show one frame with its top-left corner at the given cell, offset by
    /// `off_x` device pixels inside that cell so walking is smooth.
    ///
    /// The new placement goes up before the old one comes down, so the pet
    /// never blinks out between frames.
    pub fn show(
        &mut self,
        out: &mut impl Write,
        pet: &Pet,
        anim: &str,
        idx: usize,
        col: u16,
        row: u16,
        off_x: u32,
        off_y: u32,
        mirror: bool,
    ) -> io::Result<()> {
        let id = self.ensure(out, pet, anim, idx, mirror)?;
        let placement = self.placement;
        self.placement = self.placement % 2 + 1;

        // Save the cursor, because we have to move it to choose the cell.
        write!(out, "{ESC}7{ESC}[{};{}H", row + 1, col + 1)?;
        // C=1 keeps the cursor put, z=1 composites the pet above the text.
        write!(
            out,
            "{ESC}_Ga=p,i={id},p={placement},C=1,z=1,X={off_x},Y={off_y},q=2{ESC}\\"
        )?;
        if let Some((old_id, old_placement)) = self.shown.replace((id, placement)) {
            write!(
                out,
                "{ESC}_Ga=d,d=i,i={old_id},p={old_placement},q=2{ESC}\\"
            )?;
        }
        write!(out, "{ESC}8")?;
        out.flush()
    }

    /// Take the pet off the screen and free every image we transmitted.
    pub fn clear(&mut self, out: &mut impl Write) -> io::Result<()> {
        if let Some((id, placement)) = self.shown.take() {
            write!(out, "{ESC}_Ga=d,d=i,i={id},p={placement},q=2{ESC}\\")?;
        }
        for &id in self.sent.values() {
            write!(out, "{ESC}_Ga=d,d=I,i={id},q=2{ESC}\\")?;
        }
        self.sent.clear();
        out.flush()
    }
}

/// Device pixels per cell, from the terminal's window size.  Terminals that do
/// not report pixel dimensions get a conventional 8x16 cell.
fn cell_size() -> (u32, u32) {
    match crossterm::terminal::window_size() {
        Ok(ws) if ws.width > 0 && ws.height > 0 && ws.columns > 0 && ws.rows > 0 => (
            (ws.width / ws.columns) as u32,
            (ws.height / ws.rows) as u32,
        ),
        _ => (8, 16),
    }
}

fn base64(data: &[u8]) -> String {
    const SET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for block in data.chunks(3) {
        let b = [block[0], *block.get(1).unwrap_or(&0), *block.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(SET[(n >> 18 & 63) as usize] as char);
        out.push(SET[(n >> 12 & 63) as usize] as char);
        out.push(if block.len() > 1 { SET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if block.len() > 2 { SET[(n & 63) as usize] as char } else { '=' });
    }
    out
}
