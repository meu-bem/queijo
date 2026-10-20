# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed
- On Sway the editor no longer opens black over the screen: Sway paints a black
  backdrop behind fullscreen windows, so it shows a still picture of the screen,
  dimmed, as it does on X11 without a compositor.
- On an X11 screen with no compositor (i3, bspwm, some Xfce setups), the editor
  no longer opens black: it shows a still picture of the screen, dimmed, as
  version 0.1.0 did. `queijo --doctor` says whether a compositor was found.

### Changed
- On X11 the monitors are read from the server (RandR) instead of running
  `xrandr`, which is only a fallback now.
- Drawing on the CPU, which the editor needs on GNOME with Wayland, is no longer
  forced on other desktops. `QUEIJO_SOFTWARE_GL=1` or `0` overrides the choice.
- When no clipboard tool is installed, the error names the one to install for
  your session (`wl-clipboard` on Wayland, `xclip` on X11) instead of an
  operating system error.
- The settings ask the desktop's own folder chooser (through the portal) before
  trying `zenity` or `kdialog`, so no extra program is needed to pick a folder.
- Notifications are sent straight over D-Bus, so they work without
  `notify-send`. It stays as a fallback and is now recommended, not required.

### Added
- Window mode lists every window on X11, from the window manager's own
  properties (EWMH), with a preview cut from the screen's picture. A window that
  another covers shows what is on screen there. `queijo --list-windows` prints
  what window mode would offer.
- Screen recording through the desktop portal (ScreenCast and PipeWire) on KDE
  Plasma and any other desktop that has it, with `gst-launch-1.0` doing the
  encoding. You pick the screen in the desktop's own dialog the first time, and
  Queijo remembers it. There is no countdown before that dialog.
- Screen recording (video and GIF) on X11 with any desktop, using `ffmpeg`, and on
  Sway and Hyprland, using `wf-recorder`. The pointer option does not apply to
  `wf-recorder`, which always draws it, and an old one (Ubuntu 22.04's) records at
  the compositor's own rate.
- On KDE Plasma with Wayland, screenshots come from KWin's own service, then from
  `spectacle`, then from the portal. KWin only answers programs that its launcher
  lists, so `queijo.desktop` now does; an install that puts that file only under
  `~/.local` may not be trusted, and then `spectacle` takes over.
- On Sway and Hyprland, screenshots are taken with `grim` first, which is faster
  than the portal and never asks.
- Screenshots can also come from the desktop portal, which most desktops have,
  as a fallback when the desktop's own way fails. If the portal has to save the
  picture in a normal folder (GNOME does, in Pictures), the file is left there;
  only pictures in temporary folders are deleted.
- On X11, screenshots are taken from the X server itself, pointer included, so
  `gnome-screenshot` is no longer needed there. GNOME on X11 still asks the
  Shell first.
- `queijo --doctor` prints the session, the desktop, the distribution, the
  monitors, what Queijo will use to take screenshots and which of the tools it
  needs are missing, with the command to install them.

## [0.2.0] - 2026-10-20

### Changed
- The editor no longer covers the screen with a frozen copy of it: the live
  screen shows through, only dimmed around the selection. What gets copied or
  saved is still the screen as it was when the shortcut was pressed.
- The editor opens sooner: the screen is captured while its windows start up.
- The settings window draws its own title bar, with round minimize, maximize
  and close buttons, instead of the bare frame GNOME left it with. It can still
  be dragged, maximized with a double click and resized from its edges.

### Fixed
- On Wayland the editor renders in software, so GNOME 42 composites its
  see-through windows instead of showing them black.

## [0.1.0] - 2026-10-16

First release.

### Added
- Freeze-and-select editor on every monitor: drag an area (also across
  monitors), or pick a whole screen or a window, then copy or save.
- Annotation tools: pen, line, arrow, rectangle, ellipse, marker, text and
  pixelate, with undo/redo, a colour and size palette, and Shift to constrain
  angles and shapes.
- Screencast (WebM) and animated GIF recording with a countdown, a
  screen/window/area selector, an optional mouse pointer and a top-bar
  indicator. GIFs are recorded losslessly and optimized afterwards.
- Window picker with live previews, through the bundled GNOME Shell extension.
- Settings window: global shortcuts, folders and file-name templates per format
  (PNG or JPEG for images), recording options, behaviour toggles and language.
- English and Brazilian Portuguese; the language follows the system or can be
  chosen in the settings.
- Packages for Ubuntu 22.04 (GNOME 42, Wayland): `.deb`, a `.tar.gz` with an
  installer, and the GNOME Shell extension on its own.
