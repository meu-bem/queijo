# Dependencies, by distribution

Generated from `src/doctor.rs` (`QUEIJO_UPDATE_DOCS=1 cargo test dependencies_table` rewrites it), which is also
what `queijo --doctor` checks and what the packages declare. The Alpine names are from memory: its
musl C library can't run the release binary.

## Libraries

Opened when the editor starts, so no package manager finds them as dependencies; every package lists them.

| Library | Needed on | Debian, Ubuntu (apt) | Fedora, RHEL (dnf) | openSUSE (zypper) | Arch (pacman) | Alpine (apk) |
|---|---|---|---|---|---|---|
| `libwayland-client.so.0` | Wayland | `libwayland-client0` | `libwayland-client` | `libwayland-client0` | `wayland` | `wayland-libs-client` |
| `libxkbcommon.so.0` | always | `libxkbcommon0` | `libxkbcommon` | `libxkbcommon0` | `libxkbcommon` | `libxkbcommon` |
| `libxkbcommon-x11.so.0` | X11 | `libxkbcommon-x11-0` | `libxkbcommon-x11` | `libxkbcommon-x11-0` | `libxkbcommon-x11` | `libxkbcommon-x11` |
| `libEGL.so.1` | always | `libegl1` | `libglvnd-egl` | `libglvnd` | `libglvnd` | `mesa-egl` |
| `libGL.so.1` | always | `libgl1` | `libglvnd-glx` | `libglvnd` | `libglvnd` | `mesa-gl` |
| `libX11.so.6` | X11 | `libx11-6` | `libX11` | `libX11-6` | `libx11` | `libx11` |
| `libxcb.so.1` | X11 | `libxcb1` | `libxcb` | `libxcb1` | `libxcb` | `libxcb` |
| `libXcursor.so.1` | X11 | `libxcursor1` | `libXcursor` | `libXcursor1` | `libxcursor` | `libxcursor` |
| `libXi.so.6` | X11 | `libxi6` | `libXi` | `libXi6` | `libxi` | `libxi` |
| `libXrandr.so.2` | X11 | `libxrandr2` | `libXrandr` | `libXrandr2` | `libxrandr` | `libxrandr` |

## Programs

"Required" ones are what the feature needs to work at all; the others are fallbacks or extras.

| Program | Used for | Needed on | Required | Debian, Ubuntu (apt) | Fedora, RHEL (dnf) | openSUSE (zypper) | Arch (pacman) | Alpine (apk) |
|---|---|---|---|---|---|---|---|---|
| `ffmpeg` | recording on X11, and makes GIFs from recordings | always | yes | `ffmpeg` | `ffmpeg-free` | `ffmpeg` | `ffmpeg` | `ffmpeg` |
| `wl-copy` | copies to the clipboard on Wayland | Wayland | yes | `wl-clipboard` | `wl-clipboard` | `wl-clipboard` | `wl-clipboard` | `wl-clipboard` |
| `xclip` | copies to the clipboard on X11 | X11 | yes | `xclip` | `xclip` | `xclip` | `xclip` | `xclip` |
| `gsettings` | keyboard shortcuts on GNOME, Cinnamon and MATE, and `queijo --enable-extension` | always | no | `libglib2.0-bin` | `glib2` | `glib2-tools` | `glib2` | `glib` |
| `gifsicle` | makes GIFs a little smaller | always | no | `gifsicle` | `gifsicle` | `gifsicle` | `gifsicle` | `gifsicle` |
| `tesseract` | reads the text in a selection (needs the language data you read too) | always | no | `tesseract-ocr` | `tesseract` | `tesseract-ocr` | `tesseract` | `tesseract-ocr` |
| `notify-send` | notifications, if D-Bus has none | always | no | `libnotify-bin` | `libnotify` | `libnotify-tools` | `libnotify` | `libnotify` |
| `zenity` | folder picker, if the portal has none | always | no | `zenity` | `zenity` | `zenity` | `zenity` | `zenity` |
| `gnome-screenshot` | screenshot fallback | always | no | `gnome-screenshot` | `gnome-screenshot` | `gnome-screenshot` | `gnome-screenshot` | `gnome-screenshot` |
| `gst-launch-1.0` | screen recording through the portal (with GStreamer's pipewire plugin) | Wayland | no | `gstreamer1.0-tools` | `gstreamer1` | `gstreamer-utils` | `gstreamer` | `gstreamer-tools` |
| `wf-recorder` | screen recording on Sway and Hyprland | Sway and Hyprland | yes | `wf-recorder` | `wf-recorder` | `wf-recorder` | `wf-recorder` | `wf-recorder` |
| `spectacle` | screenshot fallback on KDE | KDE Plasma | no | `kde-spectacle` | `spectacle` | `spectacle` | `spectacle` | `spectacle` |
| `grim` | screenshot fallback on Wayland | Wayland | no | `grim` | `grim` | `grim` | `grim` | `grim` |
