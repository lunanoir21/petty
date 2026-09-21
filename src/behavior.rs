//! What the pet decides to do: idle, wander, drop into a little extra, sleep --
//! and hang there patiently while you carry it around.
//!
//! The brain only produces an animation name, a position and a facing; it
//! knows nothing about how any of that gets drawn.

use std::time::{Duration, Instant, SystemTime};

/// Anything the outside world can tell the pet.  The mouse feeds this, and so
/// does `pet event`, which is how shell events (a command failed, a build
/// finished) will reach it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    Wake,
    Jump,
    Grumpy,
    Quit,
}

impl Event {
    pub fn parse(name: &str) -> Option<Event> {
        match name.trim() {
            "wake" | "poke" => Some(Event::Wake),
            "jump" => Some(Event::Jump),
            "grumpy" | "fail" => Some(Event::Grumpy),
            "quit" | "stop" => Some(Event::Quit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Idle,
    Walk,
    Sleep,
    /// a one-off animation: stretch, curious, a glance to one side
    Extra,
    /// held by the scruff, following the pointer
    Carried,
    /// let go of, on the way back down to the floor
    Falling,
}

/// Pixels per second squared; tuned so a dropped cat lands quickly but is
/// clearly falling rather than teleporting.
const GRAVITY: f32 = 1600.0;

pub struct Brain {
    pub state: State,
    /// left edge of the sprite, in device pixels
    pub x: f32,
    /// top edge of the sprite, in device pixels
    pub y: f32,
    /// where the pet's top edge sits when it is standing on the floor
    pub floor: f32,
    pub facing_left: bool,
    pub track: f32,
    pub sprite_w: f32,
    pub sprite_h: f32,
    speed: f32,
    vy: f32,
    /// the extras this pet actually has, with how long each one runs
    extras: Vec<(String, Duration)>,
    current: String,
    until: Instant,
    restless: Duration,
    rng: Rng,
}

impl Brain {
    pub fn new(
        track: f32,
        floor: f32,
        (sprite_w, sprite_h): (f32, f32),
        speed: f32,
        extras: Vec<(String, Duration)>,
    ) -> Brain {
        let mut rng = Rng::seeded();
        let x = rng.range(0, (track - sprite_w).max(1.0) as u32) as f32;
        Brain {
            state: State::Idle,
            x,
            y: floor,
            floor,
            facing_left: false,
            track,
            sprite_w,
            sprite_h,
            speed,
            vy: 0.0,
            extras,
            current: "sit".into(),
            until: Instant::now(),
            restless: Duration::ZERO,
            rng,
        }
    }

    pub fn animation(&self) -> &str {
        &self.current
    }

    pub fn resize(&mut self, track: f32, floor: f32) {
        self.track = track;
        self.floor = floor;
        self.x = self.x.clamp(0.0, (track - self.sprite_w).max(0.0));
        if matches!(self.state, State::Idle | State::Walk | State::Sleep | State::Extra) {
            self.y = floor;
        }
    }

    /// The pet's box on screen, for hit-testing the pointer.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.sprite_w, self.sprite_h)
    }

    pub fn grabbed(&self) -> bool {
        self.state == State::Carried
    }

    pub fn grab(&mut self) {
        self.state = State::Carried;
        self.current = "carry".into();
        self.vy = 0.0;
        self.restless = Duration::ZERO;
    }

    pub fn drag_to(&mut self, x: f32, y: f32) {
        self.x = x.clamp(0.0, (self.track - self.sprite_w).max(0.0));
        self.y = y.min(self.floor);
    }

    pub fn release(&mut self) {
        if self.y < self.floor {
            self.state = State::Falling;
            self.vy = 0.0;
        } else {
            self.land();
        }
    }

    fn land(&mut self) {
        self.y = self.floor;
        self.vy = 0.0;
        // A cat that has just been put down looks around before settling.
        let glance = if self.rng.chance(50) { "look_right" } else { "look_left" };
        if self.has(glance) {
            self.play_extra(glance);
        } else {
            let d = self.rng.secs(2, 4);
            self.enter(State::Idle, "sit", d);
        }
    }

    fn has(&self, name: &str) -> bool {
        self.extras.iter().any(|(n, _)| n == name)
    }

    fn play_extra(&mut self, name: &str) {
        let span = self
            .extras
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| *d)
            .unwrap_or(Duration::from_millis(900));
        self.state = State::Extra;
        self.current = name.to_string();
        self.until = Instant::now() + span;
    }

    pub fn feed(&mut self, event: Event) {
        match event {
            Event::Wake => {
                if self.state == State::Sleep {
                    self.wake();
                }
            }
            Event::Jump => {
                if self.state == State::Sleep {
                    self.wake();
                } else if self.has("jump") {
                    self.play_extra("jump");
                }
            }
            Event::Grumpy => {
                if self.has("grumpy") {
                    self.play_extra("grumpy");
                    self.until = Instant::now() + Duration::from_secs(3);
                }
            }
            Event::Quit => {}
        }
    }

    fn wake(&mut self) {
        self.restless = Duration::ZERO;
        if self.has("wake_up") {
            self.play_extra("wake_up");
        } else {
            let d = self.rng.secs(2, 5);
            self.enter(State::Idle, "sit", d);
        }
    }

    /// Advance by one frame of `dt`.
    pub fn tick(&mut self, dt: Duration) {
        let secs = dt.as_secs_f32().min(0.2);
        match self.state {
            State::Carried => return,
            State::Falling => {
                self.vy += GRAVITY * secs;
                self.y += self.vy * secs;
                if self.y >= self.floor {
                    self.land();
                }
                return;
            }
            State::Walk => {
                let step = self.speed * secs * if self.facing_left { -1.0 } else { 1.0 };
                self.x += step;
                let limit = (self.track - self.sprite_w).max(0.0);
                if self.x <= 0.0 || self.x >= limit {
                    // Turn round at the edge rather than walking off it.
                    self.x = self.x.clamp(0.0, limit);
                    self.facing_left = !self.facing_left;
                }
            }
            _ => {}
        }

        if Instant::now() < self.until {
            return;
        }

        match self.state {
            State::Idle => self.decide(),
            State::Walk | State::Extra => {
                self.restless += Duration::from_millis(500);
                let d = self.rng.secs(2, 5);
                self.enter(State::Idle, "sit", d);
            }
            State::Sleep => self.wake(),
            _ => {}
        }
    }

    /// What an idle cat does next.  Extras are common on purpose: a pet that
    /// only ever sits and walks stops being interesting within a minute.
    fn decide(&mut self) {
        self.restless += self.rng.secs(3, 6);
        if self.restless > Duration::from_secs(50) {
            let d = self.rng.secs(60, 150);
            self.enter(State::Sleep, "sleep", d);
            return;
        }
        match self.rng.range(0, 100) {
            0..=39 => {
                self.facing_left = self.rng.chance(50);
                let d = self.rng.secs(2, 6);
                self.enter(State::Walk, "walk", d);
            }
            40..=79 if !self.extras.is_empty() => {
                let i = self.rng.range(0, self.extras.len() as u32) as usize;
                let name = self.extras[i].0.clone();
                self.play_extra(&name);
            }
            _ => {
                let d = self.rng.secs(2, 5);
                self.enter(State::Idle, "sit", d);
            }
        }
    }

    fn enter(&mut self, state: State, anim: &str, for_: Duration) {
        self.state = state;
        self.current = anim.to_string();
        self.until = Instant::now() + for_;
    }
}

/// A xorshift, so the pet can be unpredictable without pulling in a crate.
pub struct Rng(u64);

impl Rng {
    pub fn seeded() -> Rng {
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        Rng(nanos | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn range(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % (hi - lo) as u64) as u32
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        self.range(0, 100) < percent
    }

    fn secs(&mut self, lo: u32, hi: u32) -> Duration {
        Duration::from_millis(self.range(lo * 1000, hi * 1000) as u64)
    }
}
