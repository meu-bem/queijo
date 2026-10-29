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
- **Copy text from the picture**: the text button (or `O`) reads the selection with
  `tesseract`; drag over the words you want, or press `Ctrl+C` for all of them. It needs
  `tesseract` and the language data you read (`tesseract-ocr-por` for Portuguese); it reads
  the language of your session plus English, or the `ocr_languages` setting (`por+eng`) in
  `~/.config/queijo/config.toml`.
- **Freeze the screen** (Settings → General): the dim sits on a still picture of the screen, so
  what moves behind it stops. Off by default.
- **Screencast** to WebM and **animated GIF**, with a countdown, an optional mouse
  pointer and a recording indicator in the top bar. GIFs are recorded losslessly
  and then optimized (one palette per clip, light dithering, only the changed part
  of each frame).
- **Settings window**: global shortcuts, a folder and a file-name template per
  format, PNG or JPEG, recording options and language.

## Compatibility

Queijo is built for **GNOME**. It has only been tested on a real **Ubuntu 22.04 with
GNOME 42 on Wayland**, which is the environment the packages target.

It can also take screenshots and record on any X11 desktop, on KDE Plasma, on Sway and
Hyprland, and through the desktop portal. Those ways were tried only on a virtual X11
screen and a headless Sway, or against stand-ins, never on a real machine
([docs/TESTING-DESKTOPS.md](docs/TESTING-DESKTOPS.md) says what), so treat them as
experimental. Recording through the portal needs `gst-launch-1.0` with GStreamer's
PipeWire plugin, and Sway and Hyprland need `wf-recorder`.

`queijo --doctor` prints what it detected on your system and what it will use; please
include its output when you report a problem.

## Install

Download the files from the **Releases** page. They are built for x86_64 and aarch64 against
glibc 2.28, so they run on distributions as old as Debian 10, RHEL 8 and its clones, and Ubuntu
20.04. Each package was installed and run (`queijo --doctor`) on Debian 12, Ubuntu 22.04 and
24.04, Fedora, Rocky Linux 8, openSUSE Tumbleweed and Arch Linux, on x86_64. The aarch64 ones
are built the same way but have never been run.

Whatever you install, `queijo --doctor` says what Queijo found on your system, what it will use
and what is missing, with the command to install it for your distribution.
[packaging/dependencies.md](packaging/dependencies.md) lists the package names for every one.

### Ubuntu, Debian and relatives (`.deb`)

```sh
sudo apt install ./queijo_<version>-1_amd64.deb
```

`apt` also installs what Queijo needs (`ffmpeg`, a clipboard tool and a few libraries).

### Fedora, openSUSE, RHEL and relatives (`.rpm`)

```sh
sudo dnf install ./queijo-<version>-1.x86_64.rpm          # Fedora, RHEL, Rocky, Alma
sudo zypper install --allow-unsigned-rpm ./queijo-<version>-1.x86_64.rpm   # openSUSE
```

`ffmpeg` and the clipboard tool are recommended, not required, because RHEL and its clones
carry neither in their base repositories (they are in EPEL and RPM Fusion): install `ffmpeg`
and `wl-clipboard` (Wayland) or `xclip` (X11) yourself there.

### Arch Linux (AUR)

Each release includes a `PKGBUILD` and `.SRCINFO` for the AUR package `queijo-bin`. It is
not published to the AUR yet; until it is, build it yourself with `makepkg -si` in a folder
with the `PKGBUILD`.

### Any other distribution (`.tar.gz`)

```sh
tar xzf queijo-<version>-x86_64-linux.tar.gz
cd queijo-<version>-x86_64-linux
./install.sh                          # for you only (~/.local)
sudo PREFIX=/usr/local ./install.sh   # for everyone
./install.sh --uninstall              # removes it again
```

You need `ffmpeg` and a clipboard tool (`wl-clipboard` on Wayland, `xclip` on X11) installed
yourself, and the libraries `queijo --doctor` lists. `gifsicle` makes GIFs a little smaller, and
`zenity` (or `kdialog`) and `libnotify-bin` are only used when the desktop's own folder chooser
and notifications are not there.

### GNOME Shell extension (optional)

GNOME lets only the Shell see which windows exist, so a small extension provides
the window picker and the top-bar recording indicator. Since GNOME 49 it also takes
the screenshots, because the Shell no longer answers Queijo's own requests. There are
two of them, because GNOME 45 changed how extensions are written: one for GNOME 42 to
44 and one for 45 and later. The `.deb` and the `.tar.gz` include both; this turns on
the one that fits your GNOME, and then you log out and back in (on Wayland the Shell
only loads new extensions at login):

```sh
queijo --enable-extension
```

They are also released on their own as `queijo-gnome-extension-<version>-gnome42.zip`
and `-gnome45.zip` (`gnome-extensions install <file>`). Without one, window mode
captures the window that was active when Queijo opened, and recordings are stopped with
the shortcut.

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
queijo --list-windows   # the windows that window mode would offer
queijo --enable-extension  # turn on the GNOME Shell extension that fits your GNOME
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
