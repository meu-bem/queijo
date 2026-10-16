# Queijo

<img src="assets/queijo.png" width="96" align="right" alt="Queijo icon">

Screenshot, screencast and GIF tool for Linux (GNOME on Wayland or X11).

Run `queijo` (bind it to a hotkey): the screen freezes on every monitor, you drag
a region, annotate it, and copy or save. It also does what GNOME's screenshot UI
does: capture a whole screen or the active window, include the pointer, and
record screencasts.

## Modes (bar at the top)

| Key | Mode |
| --- | --- |
| `S` | **Selection**: drag any area (default) |
| `C` | **Screen**: click a monitor |
| `W` | **Window**: click the window that was active when Queijo opened |
| `V` | Switch between **screenshot** and **screencast** (video) |
| `G` | **Animated GIF** |
| `P` | Show the mouse pointer in the capture |

These keys work before anything is selected. A selection can span monitors.

In screencast mode the toolbar has a **Record** button (or press `Enter`). A
3-2-1 countdown shows how to stop, then Queijo gets out of the way and records
in the background into `~/Videos/Screencasts` (WebM). Stop by pressing the
Queijo shortcut again (or `queijo --stop`); `Esc` during the countdown cancels.
GNOME shows its orange recording indicator in the top bar meanwhile.

**GIF** works the same way. The area is recorded losslessly (FFV1, full RGB, at
25 fps — exactly GIF's 4/100 s per frame), then turned into a GIF with one
palette for the whole clip, light ordered dithering (exact UI colours untouched,
no banding in gradients) and per-frame change rectangles, and finally
`gifsicle -O3` if installed. The GIF lands next to the screencasts and is also
copied to the clipboard. Needs `ffmpeg`.

## GNOME Shell extension (window picker + recording indicator)

GNOME doesn't let other apps list windows, so Queijo ships a small Shell
extension in `gnome-extension/`. With it:

- **Window** mode shows every window on the current workspace as a preview;
  picking one captures its own pixels even if other windows cover it.
- While recording, the top bar shows **● 0:12 ■**; click it to stop.

Install it with `gnome-extension/install.sh`, then log out and back in (on
Wayland the Shell only loads new extensions at login). Without it, Window mode
falls back to the window that was active when Queijo opened, and recordings are
stopped with the shortcut only.

## Shortcuts

| Key | Action |
| --- | --- |
| Drag | Select a region (drag inside to move it, handles/edges to resize) |
| `Ctrl+A` | Select the whole monitor |
| `Ctrl+C` / `Enter` | Copy to clipboard and close (`Enter` starts recording in screencast mode) |
| `Ctrl+S` | Save (folder and name from the settings, or `-o DIR`) and close |
| `Esc` | Close |
| `P` `L` `A` `R` `E` `M` `T` `B` | Pen, line, arrow, rectangle, ellipse, marker, text, pixelate (once something is selected) |
| Mouse wheel | Change the size of the current tool |
| `Shift` while drawing | 45° lines, squares and circles |
| `Ctrl+Z` / `Ctrl+Shift+Z` | Undo / redo |
| Arrow keys | Nudge the selection (`Shift` = 10 px) |

## Settings

Click the gear in the editor's top bar, run `queijo --settings`, or open
**Queijo Settings** from the app grid. Everything applies immediately:

- **General**: starting mode (Selection/Screen/Window) and capture (screenshot,
  video, GIF), pointer on by default, notifications, "Ctrl+C also saves" and
  "Ctrl+S also copies".
- **Shortcuts**: global GNOME shortcuts to open Queijo, open straight in video
  or GIF mode, or save the whole screen without the editor (up to two each;
  click a slot and press the keys). GNOME's own screenshot shortcuts on the same
  key are switched off; clashes with other shortcuts are flagged.
- **Files**: folder and file-name template per format (`{date}`, `{time}`,
  `{year}`…`{second}`, `{width}`, `{height}`, `{timestamp}`), PNG or JPEG.
- **Recording**: countdown, video frame rate, GIF frame rate and maximum width.
- **Editor**: remember the last color and tool sizes, default color.

Settings live in `~/.config/queijo/config.toml`. Desktop entries are in
`packaging/` (copy them to `~/.local/share/applications/`), and the icon in
`assets/queijo.png` (brand yellow `#FFC42E`) goes to
`~/.local/share/icons/hicolor/<size>x<size>/apps/queijo.png`.

## CLI

```
queijo                 # interactive overlay
queijo --quick         # save the whole desktop without the editor
queijo -m eDP-1        # only use one monitor (see --list-monitors)
queijo -d 3            # wait 3 seconds first
queijo --stop          # stop a screencast
queijo --kind gif      # open in GIF mode (also: photo, video)
queijo --area window   # open in Window mode (also: selection, screen)
queijo --settings      # settings window
```

Needs `wl-clipboard` (Wayland) or `xclip` (X11) for copying. On GNOME the
capture goes straight through GNOME Shell's D-Bus API (no flash or sound),
falling back to `gnome-screenshot` and `grim`.

## Install

```
cargo build --release
install -m755 target/release/queijo ~/.local/bin/
```
