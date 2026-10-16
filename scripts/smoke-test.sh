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
