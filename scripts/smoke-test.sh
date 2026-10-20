#!/bin/sh
# Starts the settings window, in each language, under a virtual display and
# checks that it stays up, prints no panic and really draws something.
#   xvfb-run -a scripts/smoke-test.sh target/release/queijo
set -eu
bin="${1:?usage: smoke-test.sh BINARY}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

export XDG_CONFIG_HOME="$work/config" QUEIJO_NO_NOTIFY=1 LIBGL_ALWAYS_SOFTWARE=1
unset WAYLAND_DISPLAY
mkdir -p "$XDG_CONFIG_HOME/queijo"

for language in en pt-BR; do
    printf 'language = "%s"\n' "$language" > "$XDG_CONFIG_HOME/queijo/config.toml"
    log="$work/$language.log"
    "$bin" --settings > "$log" 2>&1 &
    pid=$!
    sleep 5
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "FAIL ($language): the settings window exited early"
        cat "$log"
        exit 1
    fi
    if command -v import >/dev/null 2>&1; then
        mean=$(import -window root png:- | convert png:- -colorspace Gray -format '%[fx:mean]' info:)
        echo "$language: screen brightness $mean"
        if [ "$(echo "$mean < 0.003" | bc -l)" = 1 ]; then
            echo "FAIL ($language): nothing was drawn"
            cat "$log"
            exit 1
        fi
    fi
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    if grep -qi "panicked" "$log"; then
        echo "FAIL ($language): panic"
        cat "$log"
        exit 1
    fi
    echo "ok ($language)"
done

# Record a few seconds the way the editor does (a separate process), stop it the way
# the shortcut does, and look at the video. Needs ffmpeg with x11grab, and ffprobe.
# QUEIJO_REQUIRE_RECORDING=1 makes a missing tool a failure (CI does).
if command -v ffmpeg >/dev/null 2>&1 && command -v ffprobe >/dev/null 2>&1; then
    mkdir -p "$work/videos"
    printf '[videos]\nfolder = "%s"\nname = "clip"\n' "$work/videos" > "$XDG_CONFIG_HOME/queijo/config.toml"
    "$bin" --record 0,0,320,240,0,0 > "$work/record.log" 2>&1 &
    recorder=$!
    sleep 4
    "$bin" --stop
    if ! wait "$recorder"; then
        echo "FAIL (recording): the recorder failed"
        cat "$work/record.log"
        exit 1
    fi
    video=""
    for video in "$work"/videos/*.webm; do break; done
    shape=$(ffprobe -v error -show_entries stream=width,height -of csv=p=0 "$video" 2>/dev/null | head -1)
    if [ "$shape" != "320,240" ]; then
        echo "FAIL (recording): the video is '$shape'"
        cat "$work/record.log"
        exit 1
    fi
    echo "ok (recording: $shape)"
elif [ -n "${QUEIJO_REQUIRE_RECORDING:-}" ]; then
    echo "FAIL: ffmpeg and ffprobe are needed to check recording"
    exit 1
else
    echo "skipping the recording check: ffmpeg or ffprobe is missing"
fi

# The editor on a screen with no compositor (this virtual one has none) can't be
# see-through, so it must show a still picture of the screen, dimmed, and not black.
# Needs ImageMagick (to paint and to look).
# Set QUEIJO_REQUIRE_FROZEN=1 to make a missing tool a failure (CI does).
if ! command -v display >/dev/null 2>&1 || ! command -v import >/dev/null 2>&1; then
    if [ -n "${QUEIJO_REQUIRE_FROZEN:-}" ]; then
        echo "FAIL: ImageMagick is needed for the editor check"
        exit 1
    fi
    echo "skipping the editor check: ImageMagick is missing"
    exit 0
fi
printf 'language = "en"\n' > "$XDG_CONFIG_HOME/queijo/config.toml"
display -backdrop -background '#ff0000' xc:'#ff0000' &
backdrop=$!
sleep 1
"$bin" > "$work/editor.log" 2>&1 &
pid=$!
red=0
for _ in $(seq 1 30); do
    sleep 1
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "FAIL (editor): it exited early"
        cat "$work/editor.log"
        exit 1
    fi
    red=$(import -window root png:- | convert png:- -format '%[fx:mean.r]' info:)
    # Red dimmed to about half; a black screen is 0, an undimmed one is 1.
    if [ "$(echo "$red > 0.3 && $red < 0.7" | bc -l)" = 1 ]; then
        break
    fi
done
kill "$pid" "$backdrop"
wait "$pid" 2>/dev/null || true
if [ "$(echo "$red > 0.3 && $red < 0.7" | bc -l)" != 1 ]; then
    echo "FAIL (editor): the screen is not the dimmed picture (red is $red)"
    cat "$work/editor.log"
    exit 1
fi
if grep -qi "panicked" "$work/editor.log"; then
    echo "FAIL (editor): panic"
    cat "$work/editor.log"
    exit 1
fi
echo "ok (editor without a compositor: red $red)"
