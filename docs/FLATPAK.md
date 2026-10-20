# Queijo as a Flatpak: a viability study

This is a study. Nothing was packaged and there is no manifest. It ends with a decision
for the maintainer (see "Decision needed"); no manifest should be written before it.

Every claim says how it is known: **tested** (run here, on Ubuntu 22.04 with GNOME 42 on
Wayland, Flatpak 1.12.7, the KDE Platform 6.8 runtime, which is built on the Freedesktop 24.08
one), **code** (read in this repository), or **docs** (what Flatpak's documentation says;
not tried here).

## Summary

- A Flatpak of Queijo is possible only in a reduced form that depends on desktop **portals**.
  The full feature set is not possible within what Flathub accepts, because the features that
  make Queijo fast and complete (the GNOME Shell's screenshot service, the window picker, the
  shortcuts it writes, the lists of windows on Sway and Hyprland) all reach for things a
  sandbox deliberately hides.
- Two things the study found that would break even a portals-only build, and that are not about
  portals at all: the **single-instance socket** (so "press the shortcut again to stop
  recording" stops working) and the **shortcut command** (it names a path inside the sandbox).
- The window opens in the sandbox. The first try failed for want of the GL driver extension,
  which this machine lacked; with `org.freedesktop.Platform.GL.default//24.08` installed it opens
  and stays up. (That branch is end-of-life, so a real manifest would aim at a newer runtime.)
  What has still not been seen is the editor's full-screen overlay and the portal's answer to a
  sandboxed screenshot request, both of which need a person at the screen.
- On this GNOME, the portals-only shape cannot take a screenshot: the portal refused the sandboxed
  request (tested, after 11 seconds). Option A would first need another way to capture (the
  portal's interactive mode, which shows GNOME's own screenshot interface, or a frame from the
  ScreenCast portal, which asks every time unless the remembered token covers it), and neither is
  built or tried.
- Recommendation, after that result: **do not start a Flatpak now.** With the screenshot refused,
  the window picker gone, the shortcuts by hand and the stop-by-shortcut needing a redesign, the
  Flatpak would be a different, weaker product than the native packages, which already cover the
  distributions tested. Revisit it if there is demand, starting from the screenshot question.

## What was tried

A throwaway app directory (`flatpak build-init`, the release binary built against glibc 2.28
copied into `/app/bin`) was run with `flatpak build`, which applies the same sandbox, permissions
and bus filtering as an installed app. Nothing was installed.

| Question | Result | How |
| --- | --- | --- |
| Does the binary start in the runtime? | Yes. `--doctor` runs; it sees the desktop (`XDG_CURRENT_DESKTOP` passes through), `libxkbcommon`, `libEGL`, `libGL`, `ffmpeg`, `gsettings`, `zenity`, `notify-send`. | tested |
| Are the monitors found with `--socket=wayland`? | Yes: both of them, with the right geometry and position. | tested |
| Which programs does the runtime already have? | `ffmpeg`, `ffprobe`, `gst-launch-1.0`, `gst-inspect-1.0`, `gsettings`, `notify-send`, `zenity`. **Missing:** `gifsicle`, `wl-copy`, `xclip`, `wf-recorder`, `grim`, `swaymsg`, `hyprctl`, `xrandr`. | tested |
| Does the runtime's `ffmpeg` do what Queijo asks of it? | Yes: VP8 and VP9 (`libvpx`, `libvpx-vp9`), FFV1, `gif`, `palettegen`, `paletteuse`, `scale`, `split`, `x11grab`, and the `gif`, `webm` and `matroska` muxers. | tested |
| Does its GStreamer have the recording pipeline? | Yes: `pipewiresrc`, `videoconvert`, `videoscale`, `videocrop`, `videorate`, `vp8enc`, `vp9enc`, `webmmux`, `matroskamux`, `avenc_ffv1`, `filesink`. | tested |
| Can it reach the GNOME Shell's screenshot service? | No: `ServiceUnknown` (the sandbox's bus shows only the names it was given). The screenshot chain would fall to the portal. | tested |
| Does it see the portal? | Yes, with `--talk-name=org.freedesktop.portal.Desktop` (the portal is in `--doctor`'s chain as "found"). | tested |
| Can `gsettings` write the desktop's shortcuts, or enable the extension? | No: the sandbox has no `org.gnome.shell` schema (`No such schema`), let alone the host's dconf. | tested |
| Can a second instance reach the first one's single-instance socket? | **No** in the default sandbox (`Connection refused`); yes with `--share=network`. | tested |
| Does a window open? | **Yes, once the GL extension is there.** Without `org.freedesktop.Platform.GL.default` for the runtime (this machine had none) the settings window failed with `Found no glutin configs ... provided display handle is not supported`. After installing `org.freedesktop.Platform.GL.default//24.08` for the user (with the user's agreement), the same command opened the window and it stayed up until it was stopped, with nothing on stderr. Only the settings window was tried, not the full-screen editor, as that would dim the real screens and ask the portal for a screenshot. | tested |
| What does the portal answer to a sandboxed app's screenshot request? | **A refusal, after 11 seconds**: `queijo --quick` in the sandbox (app id `app.queijo.Test`, the same GNOME 42 desktop) went Shell (`ServiceUnknown`), extension (`ServiceUnknown`), portal ("the portal did not allow the screenshot"), `gnome-screenshot` (not in the sandbox), and saved nothing. The same refusal and delay as the native build's. Whether a permission dialog showed up and went unanswered during those seconds was not observed. | tested |
| Does a file written by the host (the Shell, the extension) reach the sandbox? | Not tested: `flatpak build` does not create the per-app runtime folder. | docs |

## Feature by feature

"Portals only" means what option A below asks for; "host access" means a permission Flathub's
reviewers normally refuse.

| Feature | In the sandbox | What it needs |
| --- | --- | --- |
| The editor: dimming, selecting, annotating, copying, saving | Probably as native (GL unverified). Code is the same; it needs `--socket=wayland`, `--device=dri`. | portals only |
| Screenshot, GNOME | Portal only. The Shell service is unreachable (tested), and **GNOME 42's portal refused Queijo's non-interactive request** from the sandbox as it does from the native build (tested). So on this GNOME a portals-only Queijo takes no screenshots at all until it asks differently. Newer GNOME versions may behave otherwise (not tried). | portal; or host access to `org.gnome.Shell` for the extension's `Screenshot` (needed on GNOME 49) |
| Screenshot, KDE | Portal. KWin's own service answers only programs whose `.desktop` it finds by executable path, which a sandboxed process does not have the same way. Expected not to work (docs/code, untested). | portal |
| Screenshot, Sway and Hyprland | Portal, or `grim` bundled (it speaks a Wayland protocol and the Wayland socket exposes it). | portal; or bundle `grim` |
| Screenshot, X11 | Native: it reads the X server's root window. | `--socket=x11` (not only `fallback-x11`) |
| The pointer in a screenshot | Only on the GNOME Shell and X11 paths. Lost on the portal path. | |
| Window picker, GNOME | The extension lives in the host's Shell and cannot be installed by a Flatpak; its D-Bus interface is on `org.gnome.Shell`. | host access to `org.gnome.Shell`, and the user installs the extension by hand |
| Window picker, X11 | Native EWMH. A Wayland session's XWayland shows only X windows. | `--socket=x11` |
| Window picker, Sway and Hyprland | `swaymsg` and `hyprctl` are not in the sandbox and their sockets are in the host's runtime folder. | host access to those sockets, and Queijo would speak their IPC itself |
| Window picker, KDE | Only the active window, through KWin: as above. | |
| Recording through the portal | Plausible: the portal hands over the PipeWire connection, and the runtime has the GStreamer elements (tested). The editor-less recorder process is the same binary. | portals only |
| Recording on X11 | The runtime's `ffmpeg` has `x11grab` (tested). | `--socket=x11` |
| Recording on Sway and Hyprland | `wf-recorder` is not in the runtime. | bundle it |
| Recording on GNOME through the Shell | Needs the Shell's screencast service on the bus. | host access to `org.gnome.Shell.Screencast` |
| GIFs | The runtime's `ffmpeg` has every filter used (tested). `gifsicle` is optional and not in the runtime. | bundle `gifsicle` (small) |
| Copying to the clipboard | Done by `wl-copy` or `xclip`, which are not in the runtime. Whether `wl-copy`'s hidden window works from a sandbox on every compositor is untested. | bundle both |
| Notifications | Queijo calls `org.freedesktop.Notifications` directly, and `notify-send` (in the runtime) does the same. | `--talk-name=org.freedesktop.Notifications`, or move to the Notification portal |
| The tray icon while recording | The StatusNotifierItem protocol is allowed to Flatpaks. | `--talk-name=org.kde.StatusNotifierWatcher`, `--own-name=org.kde.StatusNotifierItem-*` |
| Global shortcuts | Cannot be written: no schema, no dconf (tested). Queijo would have to show the lines to add by hand, as it already does on Sway and KDE, with the Flatpak's command (`flatpak run <id>`), not the path inside the sandbox. | portals only (manual mode) |
| Pressing the shortcut again to stop a recording | **Broken**: the second `flatpak run` is another sandbox with its own network namespace and cannot reach the abstract socket (tested). | move the socket to a file in the app's shared runtime folder (docs), or `--share=network` (tested) |
| Where files are saved | `~/Pictures` and `~/Videos/Screencasts` need permission. A folder chosen in the file chooser is granted for the session only; keeping it across runs is not free. | `--filesystem=xdg-pictures`, `--filesystem=xdg-videos` |
| The settings | Kept in the app's own config folder (`~/.var/app/<id>/config/queijo/`), so the native install's settings are not carried over. | |
| `queijo --doctor` | Works, but names the distribution "KDE Flatpak runtime" and suggests `apt install ...`, which means nothing there. | a Flatpak-aware message |
| `queijo --enable-extension` | Cannot work (tested: no schema). | |

## The two shapes

### A. Portals only (what Flathub would accept)

```
--socket=wayland
--socket=fallback-x11
--device=dri
--share=ipc
--filesystem=xdg-pictures
--filesystem=xdg-videos
--talk-name=org.freedesktop.portal.Desktop      # (portals need no entry; listed because the doctor asks)
--talk-name=org.freedesktop.Notifications       # or use the Notification portal instead
--talk-name=org.kde.StatusNotifierWatcher
--own-name=org.kde.StatusNotifierItem-*
```

What a user gets: the editor as it is; screenshots and recordings through the portals (a
permission or a screen-picking dialog the first time, remembered afterwards by the token Queijo
already keeps); GIFs; the tray icon; no window picker except on an X11 session if `--socket=x11`
is added; shortcuts set by hand. The instant, silent screenshot of the native build on GNOME
becomes whatever the portal does.

### B. With host access (a self-hosted repository, not Flathub)

A plus: `--socket=x11`, `--talk-name=org.gnome.Shell`, `--talk-name=org.gnome.Shell.Screenshot`,
`--talk-name=org.gnome.Shell.Screencast`, `--own-name=org.gnome.Screenshot` (GNOME up to 48; GNOME
49 removed that name from the Shell's list), `--talk-name=org.kde.KWin`,
`--filesystem=xdg-run/sway-ipc.*`, `--filesystem=xdg-run/hypr`, and for shortcuts
`--talk-name=ca.desktop.dconf` with a rewrite to talk to dconf directly (the schemas are missing).
It keeps most of the native experience and is not what Flathub's review allows; its value is
mostly for people who already use Flatpak for everything and are willing to add the extension
by hand.

## What would have to change in Queijo

Small and local:

- `shortcut/manual.rs`: a Flatpak kind that writes `flatpak run <id>` instead of the program's
  path; `shortcut::apply` and the GNOME layout must not be used inside a sandbox.
- `main.rs` single-instance socket and `record::STOP`: a socket in a file under
  `$XDG_RUNTIME_DIR/app/$FLATPAK_ID` (docs: the one folder both the sandbox and the host see) when
  `FLATPAK_ID` is set. It would also be a harmless change outside Flatpak.
- `capture.rs` and `shell_ext.rs`: the files the Shell or the extension write (paths given in D-Bus
  calls) must be under that same shared folder, not `XDG_RUNTIME_DIR` as seen from inside (docs;
  only needed for option B).
- `doctor.rs`: when `FLATPAK_ID` is set, say so, drop the `apt install` advice, and point out
  which permission a missing feature would need.
- `output.rs`: the Notification portal instead of the direct call (optional).
- `shell_ext.rs` and `main.rs`: `--enable-extension` and the doctor's hint to run it, replaced by the
  extension zip's instructions.

Larger:

- A portal-based screenshot that is pleasant on GNOME: today's portal call is non-interactive and
  was refused on GNOME 42. The alternative (grabbing a frame from the ScreenCast portal) shows a
  dialog every time unless the remembered token covers it.
- Native IPC for Sway and Hyprland, if the picker should work there under option B.

## What would have to be bundled

`gifsicle`, `wl-clipboard`, `xclip`, and for Sway and Hyprland `wf-recorder` and `grim`: small
C programs with few dependencies. `ffmpeg` and GStreamer are in the runtime (tested on the KDE
Platform, which is built on the Freedesktop one; the Freedesktop Platform itself has not been
probed and may differ).

## Flathub's rules that matter here

- **The app id.** The project uses `queijo.app` for its extension and D-Bus names. Flathub accepts an
  id under a domain only if the maintainer controls it; otherwise the usual choice is
  `io.github.meu_bem.queijo`. Whether `queijo.app` is owned is not known.
- **Permissions are reviewed.** Option A is within what is usually accepted. Option B's
  `org.gnome.Shell*`, `org.kde.KWin` and `own-name=org.gnome.Screenshot` are the kind that are
  refused.
- **Build from source, offline.** The crates must be listed (`flatpak-cargo-generator`) and the
  toolchain is the runtime's Rust extension; the project's pinned Rust version has to be available.
- **Metadata.** An AppStream file with screenshots, release notes and an OARS rating; the desktop
  file and icon named after the id. The repository's `.desktop` files use `queijo`, so they would be
  renamed or patched at build time.
- **Two architectures.** The aarch64 build exists for the other formats but nothing has run it.

## Decision needed

1. Is a Flathub presence wanted, or a self-hosted Flatpak, or neither for now?
   My recommendation is **option A, and only after the GL question below is answered**; if the
   native packages already cover the audience, nothing is lost by waiting.
2. Which app id? (`io.github.meu_bem.queijo` unless `queijo.app` is yours.)
3. (Settled.) The GL extension was installed for the user and the window opens. It can be removed
   with `flatpak --user uninstall org.freedesktop.Platform.GL.default//24.08` and
   `flatpak --user remote-delete flathub`.
4. (Settled.) The portal's answer to a sandboxed screenshot request on this GNOME is a refusal after 11
   seconds. What is not known is whether a newer GNOME, or KDE, answers otherwise, or whether a
   permission dialog was shown meanwhile.

If the answer to the first question is still yes, the order would be: a capture path that the
portal accepts (the interactive portal or a ScreenCast frame), tried on GNOME and KDE; the small
code changes above (they do not depend on Flatpak existing); the manifest, the AppStream file and
the cargo sources, on a runtime that is not end-of-life; a test on a real installation; only then
submit.
