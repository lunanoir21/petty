//! The animation loop.  Step one: a clock, a player, and the preview command.
//! The behaviour state machine lands on top of this in step two.

use crate::pet::Pet;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Print;
use crossterm::{cursor, queue, terminal};
use crate::render::{Depth, Screen};
use crate::term::Term;
use std::io::Write;
use std::time::{Duration, Instant};

/// Plays one animation of a pet, advancing frames on its own clock.
pub struct Player<'a> {
    pub pet: &'a Pet,
    name: String,
    frame: usize,
    last: Instant,
}

impl<'a> Player<'a> {
    pub fn new(pet: &'a Pet, name: &str) -> Player<'a> {
        Player {
            pet,
            name: name.to_string(),
            frame: 0,
            last: Instant::now(),
        }
    }

    pub fn play(&mut self, name: &str) {
        if self.name != name {
            self.name = name.to_string();
            self.frame = 0;
            self.last = Instant::now();
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn index(&self) -> usize {
        self.frame
    }

    /// True once the animation has wrapped around to its first frame.
    pub fn tick(&mut self) -> bool {
        let anim = self.pet.animation_or_sit(&self.name);
        let step = Duration::from_secs_f32(1.0 / anim.fps);
        let mut wrapped = false;
        while self.last.elapsed() >= step {
            self.last += step;
            self.frame += 1;
            if self.frame >= anim.frames.len() {
                self.frame = if anim.looping { 0 } else { anim.frames.len() - 1 };
                wrapped = true;
            }
        }
        wrapped
    }

    pub fn draw(&self, screen: &mut Screen, x: i32, y: i32, mirror: bool) {
        let anim = self.pet.animation_or_sit(&self.name);
        let frame = &anim.frames[self.frame.min(anim.frames.len() - 1)];
        screen.blit(self.pet, frame, x, y, mirror);
    }
}

/// Input the pet reacts to.  Shell events (a failed command, say) will be fed
/// in here later; the loop does not care where an event came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Input {
    Quit,
    Jump,
    Wake,
    Resize,
    None,
}

/// Non-blocking input poll, waiting at most `timeout`.
pub fn poll(timeout: Duration) -> std::io::Result<Input> {
    if !event::poll(timeout)? {
        return Ok(Input::None);
    }
    Ok(match event::read()? {
        Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
            KeyCode::Char('c') | KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                Input::Quit
            }
            KeyCode::Char('q') | KeyCode::Esc => Input::Quit,
            KeyCode::Char(' ') => Input::Jump,
            _ => Input::Wake,
        },
        Event::Resize(..) => Input::Resize,
        _ => Input::None,
    })
}

/// `pet preview <id> <animation>` -- loop one animation so the frames can be
/// checked one by one.
pub fn preview(pet: &Pet, anim: &str, fullscreen: bool) -> std::io::Result<()> {
    // "all" walks through every animation the pet has, a few seconds each.
    let reel: Vec<String> = if anim == "all" {
        pet.animations.keys().cloned().collect()
    } else {
        vec![anim.to_string()]
    };
    let mut showing = 0usize;
    let mut switched = Instant::now();

    let rows = (pet.frame.height as u16).div_ceil(2) + 2;
    let mut term = Term::enter(fullscreen, rows)?;
    let mut screen = Screen::new(1, 1, Depth::detect());
    let mut player = Player::new(pet, &reel[0]);
    let mut layout = None;

    loop {
        let (cols, term_rows) = term.size()?;
        let rows = if term.fullscreen { term_rows } else { rows.min(term_rows) };
        if layout != Some((cols, rows)) {
            screen.resize(cols.max(1), rows.max(1));
            layout = Some((cols, rows));
            if term.fullscreen {
                queue!(term.out, terminal::Clear(terminal::ClearType::All))?;
            }
        }

        let x = (cols as i32 - pet.frame.width as i32) / 2;
        let y = (rows as i32 * 2 - pet.frame.height as i32).max(0) / 2;
        screen.clear();
        player.draw(&mut screen, x, y, false);
        screen.flush(&mut term.out, term.origin)?;

        if reel.len() > 1 && switched.elapsed() > Duration::from_secs(3) {
            showing = (showing + 1) % reel.len();
            player.play(&reel[showing]);
            switched = Instant::now();
        }

        let label = format!(
            " {} / {}  frame {}/{}   {}q to quit ",
            pet.id,
            player.name(),
            player.index() + 1,
            pet.animation_or_sit(player.name()).frames.len(),
            if reel.len() > 1 {
                format!("[{}/{}]  ", showing + 1, reel.len())
            } else {
                String::new()
            }
        );
        queue!(
            term.out,
            cursor::MoveTo(0, term.origin + rows.saturating_sub(1)),
            terminal::Clear(terminal::ClearType::UntilNewLine),
            Print(label)
        )?;
        term.out.flush()?;

        match poll(Duration::from_millis(16))? {
            Input::Quit => break,
            _ => {}
        }
        player.tick();
    }
    Ok(())
}

/// `pet` / `pet --inline` -- step one just breathes in place at the bottom of
/// the pane.  Step two replaces this body with the behaviour state machine.
pub fn run(pet: &Pet, fullscreen: bool, inline_rows: u16) -> std::io::Result<()> {
    let mut term = Term::enter(fullscreen, inline_rows)?;
    let mut screen = Screen::new(1, 1, Depth::detect());
    let mut player = Player::new(pet, "sit");
    let mut layout = None;

    loop {
        let (cols, term_rows) = term.size()?;
        let rows = if term.fullscreen { term_rows } else { inline_rows.min(term_rows) };
        if layout != Some((cols, rows)) {
            screen.resize(cols.max(1), rows.max(1));
            layout = Some((cols, rows));
            if term.fullscreen {
                queue!(term.out, terminal::Clear(terminal::ClearType::All))?;
            }
        }

        let x = (cols as i32 - pet.frame.width as i32) / 2;
        let y = rows as i32 * 2 - pet.frame.height as i32;
        screen.clear();
        player.draw(&mut screen, x, y, false);
        screen.flush(&mut term.out, term.origin)?;

        if poll(Duration::from_millis(16))? == Input::Quit {
            break;
        }
        player.tick();
    }
    Ok(())
}
