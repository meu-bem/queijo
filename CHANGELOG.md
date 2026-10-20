# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- The settings ask the desktop's own folder chooser (through the portal) before
  trying `zenity` or `kdialog`, so no extra program is needed to pick a folder.
- Notifications are sent straight over D-Bus, so they work without
  `notify-send`. It stays as a fallback and is now recommended, not required.

### Added
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
