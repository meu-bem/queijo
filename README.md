# Queijo

<img src="assets/queijo.png" width="110" align="right" alt="Queijo icon">

Screenshot, screencast and GIF tool for GNOME on Linux.

Press a shortcut and every monitor dims, while you keep seeing the live screen.
Drag over what you want, annotate it, then copy or save it: you get the screen as
it was the moment you pressed the shortcut. Switch to video or GIF and record the same
selection instead.

## Languages

| Language | |
| --- | --- |
| English | this page |
| Português (Brasil) | [README.pt-BR.md](README.pt-BR.md) |

The program itself is available in the same languages and follows your system
language; you can also pick one in **Settings → General → Language**.
More languages are planned; the way to add one is described in
[docs/TRANSLATING.md](docs/TRANSLATING.md).

## Features

- **Dim and select** on all monitors at once. A selection can span monitors.
- **Three ways to capture**: any area, a whole screen, or a window. With the GNOME
  Shell extension the window mode lists every window with a live preview.
- **Annotate**: pen, line, arrow, rectangle, ellipse, marker, text and pixelate,
  with undo/redo, colours and sizes. `Shift` keeps angles and shapes straight.
- **Screencast** to WebM and **animated GIF**, with a countdown, an optional mouse
  pointer and a recording indicator in the top bar. GIFs are recorded losslessly
  and then optimized (one palette per clip, light dithering, only the changed part
  of each frame).
- **Settings window**: global shortcuts, a folder and a file-name template per
  format, PNG or JPEG, recording options and language.

## Compatibility

Queijo is built for **GNOME**. It has only been tested on **Ubuntu 22.04 with
GNOME 42 on Wayland**, which is the environment the packages target. Other GNOME
versions, X11 sessions and other desktops are untested. Outside GNOME there is no
screen recording, and screenshots only work through the fallbacks (`gnome-screenshot`
or `grim`). `queijo --doctor` prints what it detected on your system; please
include its output when you report a problem.

## Install

Download the files from the **Releases** page.

### Ubuntu 22.04 (`.deb`)

```sh
sudo apt install ./queijo_<version>-1_amd64.deb
```

`apt` also installs what Queijo needs (`ffmpeg`, `wl-clipboard` and a few libraries).

### Any other install (`.tar.gz`)

```sh
tar xzf queijo-<version>-x86_64-linux.tar.gz
cd queijo-<version>-x86_64-linux
./install.sh                          # for you only (~/.local)
sudo PREFIX=/usr/local ./install.sh   # for everyone
./install.sh --uninstall              # removes it again
```

You need `ffmpeg` and `wl-clipboard` (Wayland) installed yourself. `gifsicle`
makes GIFs a little smaller, and `zenity` (or `kdialog`) and `libnotify-bin` are
only used when the desktop's own folder chooser and notifications are not there.

### GNOME Shell extension (optional)

GNOME lets only the Shell see which windows exist, so a small extension provides
the window picker and the top-bar recording indicator. The `.deb` and the `.tar.gz`
include it; enable it and log out and back in (on Wayland the Shell only loads new
extensions at login):

```sh
gnome-extensions enable queijo@queijo.app
```

It is also released on its own as `queijo-gnome-extension-<version>.zip`
(`gnome-extensions install <file>`). Without it, window mode captures the window
that was active when Queijo opened, and recordings are stopped with the shortcut.

## Use

Bind `queijo` to a key (for example `Print Screen`) in **Settings → Shortcuts**,
or in GNOME's own keyboard settings, then press it.

The bar at the top switches what you capture:

| Key | |
| --- | --- |
| `S` | **Selection**: drag any area (the default) |
| `C` | **Screen**: click a monitor |
| `W` | **Window**: click a window |
| `V` | **Screenshot** ⇄ **screencast** (video) |
| `G` | **Animated GIF** |
| `P` | Show the mouse pointer in the capture |

These keys work before anything is selected.

| Key | Action |
| --- | --- |
| Drag | Select an area; drag inside to move it, handles and edges to resize |
| `Ctrl+A` | Select the whole screen |
| `Ctrl+C` / `Enter` | Copy and close (`Enter` starts recording in video and GIF mode) |
| `Ctrl+S` | Save and close |
| `Esc` | Close |
| `P` `L` `A` `R` `E` `M` `T` `B` | Pen, line, arrow, rectangle, ellipse, marker, text, pixelate (once an area is selected) |
| Mouse wheel | Size of the current tool |
| `Shift` while drawing | 45° lines, squares, circles and straight marker strokes |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| Arrow keys | Nudge the selection (`Shift` = 10 px) |

### Recording

In video or GIF mode, pick the area and press **Record** (or `Enter`). A countdown
shows how to stop, then Queijo gets out of the way and records in the background.
Stop it by pressing the Queijo shortcut again, by clicking the indicator in the top
bar (with the extension), or with `queijo --stop`. `Esc` during the countdown
cancels.

Videos are WebM; GIFs are copied to the clipboard as well as saved. Both go to
`~/Videos/Screencasts` unless you change it.

### Command line

```sh
queijo                  # open the editor
queijo --quick          # save the whole desktop, without the editor
queijo --kind gif       # open in GIF mode (also: photo, video)
queijo --area window    # open in window mode (also: selection, screen)
queijo -m eDP-1         # only use one monitor (see --list-monitors)
queijo -d 3             # wait 3 seconds first
queijo -o ~/Shots       # save screenshots there this time
queijo --stop           # stop a recording
queijo --settings       # open the settings
queijo --doctor         # what Queijo found on your system, and what is missing
```

If the editor's windows show up black or the dimming is missing, set
`QUEIJO_SOFTWARE_GL=1` to draw with the CPU, or `QUEIJO_SOFTWARE_GL=0` to use the
GPU (outside GNOME, the GPU is the default). Please tell us which desktop it was.

## Settings

Open them from the gear in the editor's top bar, with `queijo --settings`, or from
**Queijo Settings** in the application grid. Changes apply immediately.

- **General**: starting mode and capture, pointer on by default, notifications,
  "Ctrl+C also saves a file", "Ctrl+S also copies", language.
- **Shortcuts**: global shortcuts to open Queijo, to open straight in video or GIF
  mode, and to save the whole screen without the editor. Up to two keys each; click
  a slot and press the keys. GNOME's own screenshot shortcuts on the same key are
  switched off, and clashes with your other shortcuts are flagged.
- **Files**: folder and file-name template for screenshots, videos and GIFs, and
  PNG or JPEG. Placeholders: `{date}` `{time}` `{year}` `{month}` `{day}` `{hour}`
  `{minute}` `{second}` `{width}` `{height}` `{timestamp}`. Existing files are never
  overwritten.
- **Recording**: countdown, video frame rate, GIF frame rate and maximum width.
- **Editor**: remember the last colour and tool sizes, starting colour, reset.

They are stored in `~/.config/queijo/config.toml`.

## Build from source

You need a recent stable Rust toolchain.

```sh
cargo build --release
cargo test
```

Installable packages are built with `scripts/build-packages.sh` (it needs
`cargo-deb` and `zip`). How a release is made is described in
[docs/RELEASING.md](docs/RELEASING.md).

## License

[MIT](LICENSE)
