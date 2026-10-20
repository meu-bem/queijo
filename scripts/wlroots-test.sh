#!/bin/sh
# Starts a headless Sway (a wlroots compositor with a red wallpaper) and checks that
# Queijo finds its monitor, takes a screenshot of it with grim, and opens an editor that
# shows a dimmed still picture (Sway can't show the live screen through a window).
# Needs sway, swaybg, grim, ImageMagick, and for the recording wf-recorder and ffprobe
# (QUEIJO_REQUIRE_RECORDING=1 makes those a requirement).
#   scripts/wlroots-test.sh target/release/queijo
set -eu
bin="${1:?usage: wlroots-test.sh BINARY}"
work=$(mktemp -d)
# Short, because a socket's path can't be longer than 107 bytes.
runtime=$(mktemp -d /tmp/queijo-wl.XXXXXX)
sway=""
editor=""
cleanup() {
    [ -z "$editor" ] || kill "$editor" 2>/dev/null || true
    [ -z "$sway" ] || kill "$sway" 2>/dev/null || true
    rm -rf "$work" "$runtime"
}
trap cleanup EXIT

export XDG_RUNTIME_DIR="$runtime" XDG_CONFIG_HOME="$work/config" XDG_CURRENT_DESKTOP=sway
export QUEIJO_NO_NOTIFY=1 WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
unset DISPLAY DBUS_SESSION_BUS_ADDRESS WAYLAND_DISPLAY
mkdir -p "$XDG_CONFIG_HOME/queijo"

printf 'output HEADLESS-1 resolution 1280x720 position 0 0 bg #ff0000 solid_color\nfor_window [app_id="queijo-settings"] floating enable\n' > "$work/sway.conf"
sway -c "$work/sway.conf" > "$work/sway.log" 2>&1 &
sway=$!
for _ in $(seq 1 20); do
    [ -S "$runtime/wayland-1" ] && break
    sleep 0.5
done
if [ ! -S "$runtime/wayland-1" ]; then
    echo "FAIL: sway did not start"
    cat "$work/sway.log"
    exit 1
fi
export WAYLAND_DISPLAY=wayland-1
# A real session gets this from Sway; swaymsg (and so Queijo) finds Sway through it.
SWAYSOCK=""
for SWAYSOCK in "$runtime"/sway-ipc.*.sock; do break; done
export SWAYSOCK
sleep 2

red() { convert png:- -format '%[fx:mean.r]' info:; }
between() { [ "$(echo "$1 > $2 && $1 < $3" | bc -l)" = 1 ]; }

monitors=$("$bin" --list-monitors)
echo "$monitors"
case "$monitors" in
    *"HEADLESS-1"*"1280x720+0+0"*) ;;
    *) echo "FAIL: the monitor was not found"; exit 1 ;;
esac

"$bin" --quick -o "$work/shots" > "$work/quick.log" 2>&1 || { echo "FAIL: --quick"; cat "$work/quick.log"; exit 1; }
shot=""
for shot in "$work"/shots/*.png; do break; done
size=$(identify -format '%wx%h' "$shot")
shot_red=$(convert "$shot" -format '%[fx:mean.r]' info:)
echo "screenshot: $size, red $shot_red"
[ "$size" = "1280x720" ] || { echo "FAIL: the screenshot is $size"; exit 1; }
between "$shot_red" 0.9 1.1 || { echo "FAIL: the screenshot is not the wallpaper"; exit 1; }
if grep -q "trying" "$work/quick.log"; then
    echo "FAIL: a backend failed and another was used:"
    cat "$work/quick.log"
    exit 1
fi

# Window mode lists the windows Sway has, through swaymsg: open one of Queijo's own.
"$bin" --settings > "$work/settings.log" 2>&1 &
editor=$!
sleep 4
listing=$("$bin" --list-windows)
echo "$listing"
kill "$editor" 2>/dev/null || true
case "$listing" in
    "Sway (1 windows, topmost first)"*"queijo-settings  Queijo Settings"*) ;;
    *) echo "FAIL: the window list is not Sway's one window"; exit 1 ;;
esac

"$bin" > "$work/editor.log" 2>&1 &
editor=$!
level=0
for _ in $(seq 1 30); do
    sleep 1
    if ! kill -0 "$editor" 2>/dev/null; then
        echo "FAIL: the editor exited early"
        cat "$work/editor.log"
        exit 1
    fi
    level=$(grim - | red)
    # The wallpaper dimmed to about half; black is 0 and undimmed is 1.
    if between "$level" 0.3 0.7; then
        break
    fi
done
between "$level" 0.3 0.7 || { echo "FAIL: the editor shows red $level, not the dimmed picture"; cat "$work/editor.log"; exit 1; }
if grep -qi "panicked" "$work/editor.log"; then
    echo "FAIL: panic"
    cat "$work/editor.log"
    exit 1
fi
kill "$editor" 2>/dev/null || true
editor=""

# Record a few seconds the way the editor does (a separate process), stop it the way
# the shortcut does, and look at the video. Needs wf-recorder and ffprobe.
if command -v wf-recorder >/dev/null 2>&1 && command -v ffprobe >/dev/null 2>&1; then
    mkdir -p "$work/videos"
    printf '[videos]\nfolder = "%s"\nname = "clip"\n' "$work/videos" > "$XDG_CONFIG_HOME/queijo/config.toml"
    "$bin" --record 0,0,640,360,0,0 > "$work/record.log" 2>&1 &
    recorder=$!
    sleep 4
    "$bin" --stop
    wait "$recorder" || { echo "FAIL: the recorder failed"; cat "$work/record.log"; exit 1; }
    video=""
    for video in "$work"/videos/*.webm; do break; done
    shape=$(ffprobe -v error -show_entries stream=width,height -of csv=p=0 "$video" 2>/dev/null | head -1)
    echo "video: $shape"
    [ "$shape" = "640,360" ] || { echo "FAIL: the video is $shape"; cat "$work/record.log"; exit 1; }
elif [ -n "${QUEIJO_REQUIRE_RECORDING:-}" ]; then
    echo "FAIL: wf-recorder and ffprobe are needed to check recording"
    exit 1
else
    echo "skipping the recording check: wf-recorder or ffprobe is missing"
fi
echo "ok (sway: screenshot, editor and recording, red $level)"
