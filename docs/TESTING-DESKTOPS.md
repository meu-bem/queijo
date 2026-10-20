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
