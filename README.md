# Petty

**pet + tty** — a hand-drawn pixel-art pet that lives in your terminal.

Petty draws a small animated creature and either runs it as a normal
terminal-pane app, or (in [kitty](https://sw.kovidgoyal.net/kitty/))
composites it directly over your shell's text using the kitty graphics
protocol, so it costs no rows and no scrollback. It wanders, sits, sleeps,
reacts to events, and can be picked up with the right mouse button.

Every pet is a single JSON file: frame size, palette, and one array of
character-grid frames per animation. Adding a new pet — a fox, an owl,
whatever — never requires touching the Rust code.

## Install

```sh
cargo install --path .
```

This installs a single binary named `pet`.

## Usage

```sh
pet overlay              # draw over your terminal's text (kitty only); backgrounds itself
pet overlay --fg         # same, but stay in the foreground
pet stop                 # tell the running overlay pet to go away
pet event <name>         # poke the running pet: wake, jump, grumpy

pet --pane                # fullscreen pane (good for a tmux/zellij split)
pet --inline              # small fixed-height area at the bottom of the terminal

pet preview <pet> <anim>  # loop one animation (or `all`) to check the frames
pet render <pet> <anim>   # print a single frame and exit
pet list                  # list the pets that are available
pet validate [dir]        # check every pet JSON and report problems

q or Ctrl+C exits and restores the terminal.
```

## Project structure

Pets are organised one directory per species, so the tree scales as more are
added:

```
pets/
  cat/
    black.json    -- a pet: metadata, palette, frames per animation
  fox/            -- reserved; drop fox_*.json variants here
  owl/            -- reserved; drop owl_*.json variants here

tools/
  shapes.py       -- shared pixel-art drawing primitives (ellipse, capsule, ...)
  sheet.py        -- renders a pet JSON to a PNG contact sheet, for review
  cat/
    black.py      -- the authoring script that generates pets/cat/black.json
  fox/, owl/      -- reserved, mirroring pets/
```

Every `*.json` under `pets/` (any depth) is picked up automatically, both at
build time (baked into the binary via `build.rs`) and at runtime (dropped
into `$PET_DIR`, `$XDG_DATA_HOME/pet/pets`, or `./pets` without a rebuild).

### Adding a new pet

1. Make a folder for the species if it does not exist yet: `pets/fox/`.
2. Write an authoring script under `tools/fox/` that builds the frames with
   the shared primitives in `tools/shapes.py`, and writes
   `pets/fox/<variant>.json`. Use `tools/cat/black.py` as a template.
3. Run `python3 tools/sheet.py pets/fox/<variant>.json out.png` to eyeball
   the frames as a PNG contact sheet before wiring anything up.
4. Run `pet validate pets/` and fix anything it flags.
5. `pet preview fox_<variant> <animation>` to check it frame by frame in a
   real terminal.

### Pet JSON format

```jsonc
{
  "id": "cat_black",
  "name": "Classic",
  "species": "cat",
  "frame": { "width": 80, "height": 72 },
  "transparent": ".",
  "palette": { "d": "#1a1a1f", "o": "#f2b23c", "...": "..." },
  "animations": {
    "walk": {
      "view": "side",     // "side" mirrors when facing left; "front" never does
      "fps": 14,
      "loop": true,
      "advance": 22,      // sprite pixels covered per full cycle (walk-type anims)
      "frames": [["...", "..."], ["...", "..."]]
    }
  }
}
```

Required animations: `sit`, `walk`, `sleep`. A pet may add extras
(`look_left`, `look_right`, `curious`, `grumpy`, `back_view`, `stretch`,
`jump`, `wake_up`, `carry`, ...) and the overlay behaviour picks up whichever
ones exist. Palette is capped at 8 colours.

## Reference art

`reference/` holds the original concept sheets pets were hand-drawn from.
The pixel art itself is always redrawn by hand — never traced or
downscaled from the reference image.
