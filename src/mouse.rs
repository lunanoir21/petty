//! Picking the pet up with the right mouse button.
//!
//! The overlay runs detached from the shell, so it cannot read mouse reports
//! from the terminal -- those go to whatever owns stdin.  Instead we watch the
//! mouse device directly for the right button, and ask the compositor where
//! the pointer is.  Only button events are looked at; nothing is recorded.
//!
//! Everything here is best-effort: with no readable mouse, or a compositor
//! that is not Hyprland, the pet simply cannot be picked up and carries on.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const EV_KEY: u16 = 1;
const BTN_RIGHT: u16 = 273;
/// struct input_event on 64-bit linux: two 64-bit timeval fields, then the event
const EVENT_SIZE: usize = 24;

pub struct Mouse {
    right_down: Arc<AtomicBool>,
    hypr: PathBuf,
}

impl Mouse {
    pub fn new() -> Option<Mouse> {
        let hypr = hypr_socket()?;
        let device = find_mouse()?;
        let right_down = Arc::new(AtomicBool::new(false));

        let flag = right_down.clone();
        std::thread::spawn(move || {
            let Ok(mut file) = std::fs::File::open(&device) else { return };
            let mut buf = [0u8; EVENT_SIZE * 16];
            loop {
                let Ok(n) = file.read(&mut buf) else { return };
                for event in buf[..n].chunks_exact(EVENT_SIZE) {
                    let kind = u16::from_ne_bytes([event[16], event[17]]);
                    let code = u16::from_ne_bytes([event[18], event[19]]);
                    let value = i32::from_ne_bytes([event[20], event[21], event[22], event[23]]);
                    if kind == EV_KEY && code == BTN_RIGHT {
                        flag.store(value != 0, Ordering::Relaxed);
                    }
                }
            }
        });

        Some(Mouse { right_down, hypr })
    }

    pub fn right_down(&self) -> bool {
        self.right_down.load(Ordering::Relaxed)
    }

    /// Pointer position in compositor coordinates.
    pub fn cursor(&self) -> Option<(i32, i32)> {
        let reply = self.ask("cursorpos")?;
        let v: serde_json::Value = serde_json::from_str(&reply).ok()?;
        Some((v.get("x")?.as_i64()? as i32, v.get("y")?.as_i64()? as i32))
    }

    /// Where our own terminal window sits: (x, y, width, height).
    pub fn window_rect(&self) -> Option<(i32, i32, i32, i32)> {
        let ancestors = ancestor_pids();
        let reply = self.ask("clients")?;
        let clients: serde_json::Value = serde_json::from_str(&reply).ok()?;
        for c in clients.as_array()? {
            let pid = c.get("pid")?.as_i64()? as u32;
            if !ancestors.contains(&pid) {
                continue;
            }
            let at = c.get("at")?.as_array()?;
            let size = c.get("size")?.as_array()?;
            return Some((
                at[0].as_i64()? as i32,
                at[1].as_i64()? as i32,
                size[0].as_i64()? as i32,
                size[1].as_i64()? as i32,
            ));
        }
        None
    }

    fn ask(&self, command: &str) -> Option<String> {
        let mut sock = UnixStream::connect(&self.hypr).ok()?;
        sock.write_all(format!("j/{command}").as_bytes()).ok()?;
        let mut reply = String::new();
        sock.read_to_string(&mut reply).ok()?;
        Some(reply)
    }
}

fn hypr_socket() -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let path = PathBuf::from(runtime).join("hypr").join(signature).join(".socket.sock");
    path.exists().then_some(path)
}

/// The first readable input device that reports a right mouse button.
fn find_mouse() -> Option<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/dev/input").ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }
        if std::fs::File::open(&path).is_err() {
            continue;
        }
        let caps = PathBuf::from("/sys/class/input")
            .join(&*name)
            .join("device/capabilities/key");
        if std::fs::read_to_string(caps).is_ok_and(|k| has_bit(&k, BTN_RIGHT as u32)) {
            found.push(path);
        }
    }
    found.sort();
    found.into_iter().next()
}

/// Capability bitmaps are printed as space-separated 64-bit words, high first.
fn has_bit(bitmap: &str, bit: u32) -> bool {
    let words: Vec<&str> = bitmap.split_whitespace().collect();
    let (word, offset) = (bit / 64, bit % 64);
    let Some(index) = words.len().checked_sub(word as usize + 1) else {
        return false;
    };
    u64::from_str_radix(words[index], 16).is_ok_and(|w| w >> offset & 1 == 1)
}

/// Our pid and every parent above it, so we can spot our own terminal window.
fn ancestor_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    let mut pid = std::process::id();
    for _ in 0..12 {
        pids.push(pid);
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { break };
        // The comm field can contain spaces and brackets, so read what comes
        // after the last ')': state, then ppid.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else { break };
        let Some(ppid) = rest.split_whitespace().nth(1).and_then(|p| p.parse().ok()) else { break };
        if ppid <= 1 {
            break;
        }
        pid = ppid;
    }
    pids
}
