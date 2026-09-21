mod app;
mod behavior;
mod kitty;
mod mouse;
mod overlay;
mod pet;
mod render;
mod term;

use std::process::ExitCode;

const USAGE: &str = "\
pet -- a pixel-art pet for your terminal

usage:
  pet overlay [--scale auto|N] [--lift N] [--speed N]
                                   draw the pet over your terminal's text, using
                                   no rows and no scrollback (needs kitty).  It
                                   sizes itself to the window, wanders about,
                                   and can be picked up with the right mouse
                                   button.  Runs in the background --
                                   `pet stop` ends it
  pet overlay --fg                 same, but stay in the foreground
  pet stop                         tell the running overlay pet to go away
  pet event <name>                 poke the running pet: wake, jump, grumpy
  pet --pane [--pet <id>]          fullscreen pane (good for a tmux/zellij split)
  pet --inline [--height <rows>]   a small fixed-height area at the bottom
  pet preview <pet> <animation>    loop one animation (or `all`) to check frames
  pet render <pet> <animation> [n] print a single frame and exit
  pet list                         list the pets that are available
  pet validate [dir]               check every pet JSON and report problems

  q or Ctrl+C exits and restores the terminal.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();

    match argv.first().copied() {
        Some("-h") | Some("--help") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("overlay") => cmd_overlay(&argv[1..]),
        Some("stop") => cmd_stop(),
        Some("event") => match argv.get(1) {
            Some(name) => cmd_event(name),
            None => fail("usage: pet event <wake|jump|grumpy>"),
        },
        Some("list") => cmd_list(),
        Some("validate") => cmd_validate(argv.get(1).copied()),
        Some("render") => match (argv.get(1), argv.get(2)) {
            (Some(id), Some(anim)) => {
                let n = argv.get(3).and_then(|v| v.parse().ok()).unwrap_or(0);
                cmd_render(id, anim, n)
            }
            _ => fail("usage: pet render <pet> <animation> [frame]"),
        },
        Some("preview") => match (argv.get(1), argv.get(2)) {
            (Some(id), Some(anim)) => cmd_preview(id, anim),
            _ => fail("usage: pet preview <pet> <animation>"),
        },
        _ => cmd_run(&argv),
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("pet: {msg}");
    ExitCode::FAILURE
}

fn load(id: Option<&str>) -> Result<pet::Pet, String> {
    let (mut pets, errs) = pet::load_all();
    for e in &errs {
        eprintln!("pet: warning: {e}");
    }
    let id = id.map(str::to_string).unwrap_or_else(|| {
        std::env::var("PET_NAME").unwrap_or_else(|_| "cat_black".into())
    });
    let p = pets
        .remove(&id)
        .ok_or_else(|| format!("no pet named {id:?}; try `pet list`"))?;
    let problems = p.validate();
    if !problems.is_empty() {
        return Err(format!(
            "pet {id:?} is not valid:\n  {}",
            problems.join("\n  ")
        ));
    }
    Ok(p)
}

fn cmd_list() -> ExitCode {
    let (pets, errs) = pet::load_all();
    for e in &errs {
        eprintln!("pet: warning: {e}");
    }
    for p in pets.values() {
        let mut anims: Vec<&str> = p.animations.keys().map(String::as_str).collect();
        anims.sort();
        println!(
            "{:<12} {:<10} {}x{}  {}",
            p.id,
            p.name,
            p.frame.width,
            p.frame.height,
            anims.join(" ")
        );
    }
    ExitCode::SUCCESS
}

fn cmd_validate(dir: Option<&str>) -> ExitCode {
    let mut sources: Vec<(String, String)> = Vec::new();
    if let Some(dir) = dir {
        if !std::path::Path::new(dir).is_dir() {
            return fail(&format!("{dir}: not a directory"));
        }
        let mut paths = Vec::new();
        pet::collect_json(std::path::Path::new(dir), &mut paths);
        paths.sort();
        for path in paths {
            match std::fs::read_to_string(&path) {
                Ok(s) => sources.push((path.display().to_string(), s)),
                Err(e) => return fail(&format!("{}: {e}", path.display())),
            }
        }
    } else {
        for (name, json) in pet::EMBEDDED {
            sources.push((name.to_string(), json.to_string()));
        }
    }

    if sources.is_empty() {
        return fail("no pet JSON files found");
    }

    let mut bad = 0;
    for (name, json) in &sources {
        match pet::Pet::parse(json) {
            Err(e) => {
                bad += 1;
                println!("FAIL {name}\n  {e}");
            }
            Ok(p) => {
                let problems = p.validate();
                if problems.is_empty() {
                    let frames: usize = p.animations.values().map(|a| a.frames.len()).sum();
                    println!(
                        "ok   {name}  {}x{}  {} colours  {} animations  {frames} frames",
                        p.frame.width,
                        p.frame.height,
                        p.palette.len(),
                        p.animations.len()
                    );
                } else {
                    bad += 1;
                    println!("FAIL {name}");
                    for e in problems {
                        println!("  {e}");
                    }
                }
            }
        }
    }
    if bad > 0 {
        eprintln!("\n{bad} of {} pet files failed", sources.len());
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_preview(id: &str, anim: &str) -> ExitCode {
    let p = match load(Some(id)) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    if anim != "all" && p.animation(anim).is_none() {
        let mut have: Vec<&str> = p.animations.keys().map(String::as_str).collect();
        have.sort();
        return fail(&format!(
            "{id} has no animation {anim:?}; it has: {}",
            have.join(" ")
        ));
    }
    match app::preview(&p, anim, true) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}

/// Dump one frame to stdout and exit -- handy for eyeballing a frame without
/// entering the animation loop, and for piping into a diff.
fn cmd_render(id: &str, anim_name: &str, n: usize) -> ExitCode {
    let p = match load(Some(id)) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let Some(anim) = p.animation(anim_name) else {
        return fail(&format!("{id} has no animation {anim_name:?}"));
    };
    let frame = &anim.frames[n % anim.frames.len()];
    let mut screen = render::Screen::new(p.frame.width as u16, (p.frame.height as u16).div_ceil(2), render::Depth::detect());
    screen.blit(&p, frame, 0, 0, false);
    let mut out = std::io::stdout();
    if let Err(e) = screen.print(&mut out) {
        return fail(&e.to_string());
    }
    ExitCode::SUCCESS
}

fn running_pid() -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(overlay::pid_file())
        .ok()?
        .trim()
        .parse()
        .ok()?;
    // A stale pid file outlives a crash, so check the process is really there.
    std::path::Path::new(&format!("/proc/{pid}")).exists().then_some(pid)
}

fn cmd_overlay(argv: &[&str]) -> ExitCode {
    let mut opts = overlay::Options { scale: None, lift: 0, speed: 26.0 };
    let mut foreground = false;
    let mut id: Option<&str> = None;

    let mut i = 0;
    while i < argv.len() {
        let value = |i: &mut usize| {
            *i += 1;
            argv.get(*i).copied()
        };
        match argv[i] {
            "--fg" | "--foreground" => foreground = true,
            "--scale" => match value(&mut i) {
                Some("auto") => opts.scale = None,
                Some(v) => match v.parse() {
                    Ok(v) => opts.scale = Some(v),
                    Err(_) => return fail("--scale needs a number, or `auto`"),
                },
                None => return fail("--scale needs a number, or `auto`"),
            },
            "--lift" => match value(&mut i).and_then(|v| v.parse().ok()) {
                Some(v) => opts.lift = v,
                None => return fail("--lift needs a number of rows"),
            },
            "--speed" => match value(&mut i).and_then(|v| v.parse().ok()) {
                Some(v) => opts.speed = v,
                None => return fail("--speed needs a number (pixels per second)"),
            },
            "--pet" => match value(&mut i) {
                Some(v) => id = Some(v),
                None => return fail("--pet needs a pet id"),
            },
            other => return fail(&format!("unknown argument {other:?}")),
        }
        i += 1;
    }

    if let Some(pid) = running_pid() {
        return fail(&format!("a pet is already running (pid {pid}); `pet stop` first"));
    }
    let p = match load(id) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };

    if foreground {
        return match overlay::run(&p, opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&e.to_string()),
        };
    }

    // Re-exec ourselves in the background. The child keeps this controlling
    // terminal, which is what it draws on; its stdout goes to a log so a
    // stray message can never land on the screen.
    let Ok(exe) = std::env::current_exe() else {
        return fail("cannot find my own executable to start in the background");
    };
    if let Err(e) = std::fs::create_dir_all(overlay::runtime_dir()) {
        return fail(&format!("{}: {e}", overlay::runtime_dir().display()));
    }
    let log = overlay::runtime_dir().join("pet.log");
    let Ok(out) = std::fs::File::create(&log) else {
        return fail(&format!("cannot write {}", log.display()));
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("overlay").arg("--fg");
    cmd.arg("--scale")
        .arg(opts.scale.map_or_else(|| "auto".to_string(), |s| s.to_string()));
    cmd.arg("--lift").arg(opts.lift.to_string());
    cmd.arg("--speed").arg(opts.speed.to_string());
    if let Some(id) = id {
        cmd.arg("--pet").arg(id);
    }
    cmd.stdin(std::process::Stdio::null())
        .stderr(out.try_clone().unwrap_or_else(|_| out.try_clone().unwrap()))
        .stdout(out);
    match cmd.spawn() {
        Ok(child) => {
            println!("pet is on screen (pid {}); `pet stop` to send it away", child.id());
            ExitCode::SUCCESS
        }
        Err(e) => fail(&format!("could not start the pet: {e}")),
    }
}

fn cmd_stop() -> ExitCode {
    match overlay::send("quit") {
        Ok(true) => {
            println!("asked the pet to go away");
            ExitCode::SUCCESS
        }
        Ok(false) => fail("no pet is running"),
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_event(name: &str) -> ExitCode {
    if behavior::Event::parse(name).is_none() {
        return fail(&format!("unknown event {name:?}; try wake, jump or grumpy"));
    }
    match overlay::send(name) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => fail("no pet is running"),
        Err(e) => fail(&e.to_string()),
    }
}

fn cmd_run(argv: &[&str]) -> ExitCode {
    let mut inline = false;
    let mut height: Option<u16> = None;
    let mut id: Option<&str> = None;

    let mut i = 0;
    while i < argv.len() {
        match argv[i] {
            "--inline" => inline = true,
            "--pane" | "--full" => inline = false,
            "--height" => {
                i += 1;
                match argv.get(i).and_then(|v| v.parse().ok()) {
                    Some(h) => height = Some(h),
                    None => return fail("--height needs a number of rows"),
                }
            }
            "--pet" => {
                i += 1;
                match argv.get(i) {
                    Some(v) => id = Some(v),
                    None => return fail("--pet needs a pet id"),
                }
            }
            other => return fail(&format!("unknown argument {other:?}\n\n{USAGE}")),
        }
        i += 1;
    }

    let p = match load(id) {
        Ok(p) => p,
        Err(e) => return fail(&e),
    };
    let rows = height.unwrap_or_else(|| (p.frame.height as u16).div_ceil(2));
    match app::run(&p, !inline, rows) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}
