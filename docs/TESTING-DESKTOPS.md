# Testing on other desktops

Queijo was built for GNOME and is being extended to other desktops (the plan is in
`docs/plans/portabilidade-linux.md`). Only the first row below has been tried on a
real system. A cell is one of:

- `✅` tested, with the date and who tested it
- `⏳` not tested yet
- `❌` not supported, with the reason

The table says what has been **tested**, not what has been written: nothing outside
GNOME is expected to work until the matching phase of the plan has landed. Fill the
cells in as it does, and keep `README.md`'s compatibility section in step with them
(only `✅` counts as supported).

Start every test by running `queijo --doctor` and pasting its output next to the result.

| Desktop | Overlay | Screenshot | Area | Screen | Window | Video | GIF | Clipboard | Shortcut | Indicator |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GNOME 42, Wayland (Ubuntu 22.04) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| GNOME 42, X11 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| GNOME 43–44 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| GNOME 45–49 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| KDE Plasma 5.27, Wayland | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| KDE Plasma 5.27, X11 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| KDE Plasma 6, Wayland | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| KDE Plasma 6, X11 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| Sway | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| Hyprland | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| Xfce, X11 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| Cinnamon, X11 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| MATE, X11 | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| i3 / bspwm, X11, no compositor | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |
| COSMIC | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ | ⏳ |

The GNOME 42 Wayland row is the environment the README has always claimed (Ubuntu
22.04); `--doctor` and `--quick` were run there again after the desktop detection
and the screenshot backend list were added (2026-10-20).

If the overlay is black on a desktop other than GNOME, `QUEIJO_SOFTWARE_GL=1` is the
workaround, and the fix is to include that desktop in `desktop::wants_software_gl`.
Drawing on the GPU is the untested default everywhere but GNOME on Wayland.

Sway paints a black backdrop behind every fullscreen window, so the editor, which is
fullscreen, cannot show the live screen through itself there (found with a headless Sway,
where the background came out black over a red wallpaper). It shows a still picture, as it
does without a compositor. Hyprland and the others have not been tried.

Without a compositor (the i3 / bspwm row) the editor shows a still picture of the screen
instead of the live one. `scripts/smoke-test.sh` checks that on a virtual display, which
has no compositor, and CI runs it; it still has to be looked at on a real one.

The Screenshot portal was tried on GNOME 42 (Ubuntu 22.04): it refused the request that
Queijo makes (no window, no interaction) after about ten seconds, and left a picture in
`~/Pictures`. It is only the last resort there, after the Shell. Other portals (KDE's,
wlroots') have not been tried.

KDE's screenshot service (`org.kde.KWin.ScreenShot2`) is written from KWin's own interface
description and tested against a stand-in for it, never against a real Plasma. It only
answers programs whose `.desktop` file lists the interface in
`X-KDE-DBUS-Restricted-Interfaces` (`queijo.desktop` does), found through the usual
application folders. Things to look at on a real Plasma 5.27 and 6: that the call is
accepted for the `.deb` install and for the `~/.local` one, which pixel format arrives, and
the picture's size on a screen with fractional scaling.

Recording on X11 (`ffmpeg x11grab`) and on Sway (`wf-recorder` 0.2.1) was tried end to end
on a virtual X11 screen and a headless Sway. Recording through the ScreenCast portal was
tried against a stand-in for the portal and with GStreamer's test picture in place of the
PipeWire stream (so the crop and the encoder are checked); it has never run against a real
portal or a real PipeWire screen stream, and a hand-made PipeWire node did not deliver
frames, so that part is unverified. Things to look at: that the dialog appears and the
chosen screen is what is recorded, that a remembered screen is not asked again, that an
area on another screen records the whole picked one with a notification, and fractional
scaling (the recording is made at the screen's logical size).

Window mode on KDE Plasma with Wayland only has the active window (`CaptureActiveWindow`,
documented since version 2 of KWin's screenshot interface; tested against a stand-in, never a
real KWin). Listing the others would take a script loaded into KWin (`org.kde.KWin /Scripting`
`loadScript`) that reports back over D-Bus, and `CaptureWindow` takes a window identifier that
only such a script can give; that was judged too much to write without a Plasma to try it on, so
it is left as a possible next step. Things to look at: that the active window is found on a
screen with fractional scaling, and that its title bar is in the picture.

Window mode on Sway was tried on a headless Sway with a real window: the list and the picker
work. On Hyprland it has only been checked against JSON written by hand from the fields
`hyprctl -j clients` and `-j monitors` are known to have (focus order stands in for stacking
order, and workspace and monitor are matched through `activeWorkspace`), never against a running
Hyprland. On Sway the floating windows are listed first, the last of them topmost, then the tiled.

Window mode on X11 was tried with openbox on a virtual screen: the list, its order, the
focus and the frames match what the window manager does, and the picker shows previews cut
from the screen's picture. Other window managers (and their `_NET_FRAME_EXTENTS`, for
example with client-side decorations) have not been tried. A window that another covers
shows what is on screen there, because X11 has no way to ask for its hidden pixels.

GNOME 43 and 44: the older extension now says it works with them (the code does not use
anything that changed before GNOME 45), but only GNOME 42 has run it.

GNOME 45 and later: the extension for them (`gnome-extension/esm/`) has only been run
against stand-ins for GNOME's modules (`tests/extension/`), which checks its logic and the
shape of its D-Bus interface but not that the real Shell accepts what it calls. Source checks
that were done: GNOME 48's screenshot service still allows `org.gnome.Screenshot`; GNOME 49's
allows only `org.gnome.SettingsDaemon.MediaKeys` and `org.freedesktop.impl.portal.desktop.gnome`,
which are owned by running programs and can't be borrowed. 49's own code takes a screenshot with
`new Shell.Screenshot().screenshot(includeCursor, stream)` into a file opened with
`file.replace(null, false, Gio.FileCreateFlags.NONE, null)`, which is what the extension does. Not
checked: whether the screencast service has the same restriction (the portal is the fallback), and
whether `Meta.WindowActor.get_image` still exists (newer shells use `paint_to_content`). Things
to look at on a GNOME 45 to 49: that `queijo --doctor` finds the extension after login, the
screenshot, the window picker with its previews, and the recording indicator.

Shortcuts on Sway, Hyprland, i3, KDE and the rest are only shown, not written: the lines are
checked as text against each configuration's syntax (Sway and i3 `bindsym`, Hyprland `bind =`), not
by loading them into a compositor, and the Shortcuts page with them has not been looked at on a
screen. For KDE, the way to register a global shortcut for a command (a `.desktop` file marked
`X-KDE-GlobalAccel-CommandShortcut=true`, a `[services]` section in `kglobalshortcutsrc`, then a
restart of `kglobalacceld`) was found described but its reload step was not confirmed anywhere, so
it was not implemented.

Shortcuts on Xfce (`src/shortcut/xfce.rs`) were run against the real `xfconf-query` and `xfconfd`
(from the `xfconf` package) on a bus of their own, with the shortcuts channel filled the way Xfce
fills it, so the property names, the listing and the quoting of commands are right. What was not
seen is Xfce itself reloading: that a shortcut written this way works at once and that a custom
shortcut for a key really wins over the default (`override`). Things to look at: Print, with Xfce's
screenshot tool on it, and a shortcut with a modifier.

Shortcuts on Cinnamon and MATE (`src/shortcut/lists.rs`) are written from how those desktops
keep their custom shortcuts (the schema and key names, a list of `customN` names and keys held in
a list on Cinnamon, `customN` found by trying them and an `action` key on MATE, `disabled` for a
switched-off MATE shortcut) and run only against a map standing in for their settings. The schemas
themselves were not available to check. Things to look at: that a shortcut set in Queijo's
settings appears in the desktop's own keyboard settings and works, that the desktop's screenshot
key is freed, and that removing it leaves the desktop's other shortcuts alone.

The recording tray icon (`src/record/tray.rs`, through the `ksni` crate) was run against a
stand-in for the StatusNotifierWatcher on a bus of its own: it registers, its properties are
right, and a click (`Activate`) stops the recording. It has never been shown by a real tray. Things
to look at on KDE, Xfce, Cinnamon, MATE or a bar such as waybar: that the red dot appears while
recording and goes when it stops, that the tooltip's time moves, and that the click and the menu item
stop the recording. Where there is no tray, or no host for it, Queijo says so on stderr and records
without an icon.

## What to try in each environment

1. **Overlay**: the screen dims, the live screen shows through, and the selection
   follows the mouse. On every monitor, if there are several.
2. **Screenshot / Area / Screen / Window**: select with `S`, `C` and `W`; copy with
   `Ctrl+C` and save with `Ctrl+S`; the picture is what was on screen when the
   shortcut was pressed, without the dimming.
3. **Video / GIF**: record a few seconds of moving content, stop it with the
   shortcut, and open the file.
4. **Clipboard**: paste the copied picture into another program.
5. **Shortcut**: set one in **Settings → Shortcuts** and press it.
6. **Indicator**: while recording, something shows that it is running and can stop it.

## Setting up an environment

Use the live image of the distribution in a virtual machine (GNOME Boxes or
virt-manager; enable 3D acceleration where it is offered):

| Environment | Image |
| --- | --- |
| GNOME | Ubuntu 22.04 (42), Ubuntu 24.04 (46), Fedora Workstation |
| KDE Plasma | Kubuntu 22.04 (5.24) or 24.04 (5.27), Fedora KDE (6) |
| Sway | Fedora Sway Spin, or `sway` from any distribution |
| Hyprland | Arch, or the Hyprland live images |
| Xfce / MATE / Cinnamon | Xubuntu, Ubuntu MATE, Linux Mint |
| i3 | Debian with `i3`, or Fedora i3 Spin |

Install Queijo from the `.tar.gz` (`./install.sh`) or build it with `cargo build --release`.
