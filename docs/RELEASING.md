# Branches, versions and releases

## Branches

| Branch | For | CI |
| --- | --- | --- |
| `test` | day-to-day work | formatting, lints, tests, smoke test on a virtual display, extension and launcher checks, and a build of the packages (kept as a workflow artifact for 14 days) |
| `main` | what has been released | the same checks, then the release is published |

Work on `test`. When it is ready to be released, open a pull request from `test` to
`main`; the checks run on the pull request too. Nothing else should be pushed to
`main`.

CI can also be started by hand from the Actions tab (pick the branch).

Recommended repository settings (they can't be set from the repository itself):
protect `main`, require a pull request and the **CI** checks to pass, and don't
allow direct pushes.

## Versions

The version is the one in `Cargo.toml` ([Semantic Versioning](https://semver.org/)):

- `MAJOR.MINOR.PATCH`; before 1.0.0, a breaking change raises the minor number.
- `scripts/version.sh` prints it; everything else (package names, the tag, the
  release title) is derived from it. `Cargo.lock` must carry the same number, so
  run `cargo build` after changing it; CI builds with `--locked`.
- A version with a `-` (`0.2.0-rc.1`) is published as a pre-release.

## Making a release

On `test`:

1. Raise `version` in `Cargo.toml` and build once so `Cargo.lock` follows.
2. In `CHANGELOG.md`, move the entries from **Unreleased** to a new
   `## [x.y.z] - YYYY-MM-DD` section. The text of that section is the release
   notes, and the CI check fails if the current version has none.
3. Push, wait for CI, open the pull request into `main` and merge it.

When `main` is updated, the **Release** workflow runs the checks again and then,
only if `v<version>` doesn't exist yet:

- builds the packages (`scripts/build-packages.sh`),
- installs them in containers of four distributions and runs them (`scripts/distro-test.sh`),
  so nothing is published that did not install where it is meant to,
- creates the tag `v<version>` on that commit,
- publishes the release with the notes from `CHANGELOG.md` and these files:

| File | What it is |
| --- | --- |
| `queijo_<version>-1_<amd64\|arm64>.deb` | package for Debian, Ubuntu and their relatives |
| `queijo-<version>-1.<x86_64\|aarch64>.rpm` | package for Fedora, openSUSE, RHEL and their relatives |
| `queijo-<version>-<x86_64\|aarch64>-linux.tar.gz` | the binary, launchers, icons and extensions with `install.sh`, for any distribution |
| `queijo-gnome-extension-<version>-gnome42.zip` | the GNOME Shell extension for GNOME 42 to 44, alone |
| `queijo-gnome-extension-<version>-gnome45.zip` | the one for GNOME 45 and later, alone |
| `PKGBUILD`, `.SRCINFO` | for the AUR package `queijo-bin` (`scripts/aur.sh`); publishing them there is a manual step |
| `SHA256SUMS` | checksums of the files above |

If the version was already released, the workflow ends with a notice and publishes
nothing: merging documentation or fixes to `main` without a version bump is safe.

## The Rust version

The workflows use a pinned toolchain (`RUST_VERSION` in `ci.yml` and `release.yml`),
so a new Rust release, with its new lints, can't turn CI red on its own. Raise it
on purpose, in both files, after running the checks below with that version.

## What the packages promise

The binary is built with zig as the linker (`cargo zigbuild`) against **glibc 2.28**, and
`scripts/build-packages.sh` stops if it needs a newer one. That is what makes one build run on
Debian 10, RHEL 8 and its clones, Ubuntu 20.04 and everything newer, instead of only on the
distribution it was built on. The graphics libraries are opened at run time and not linked, so
glibc is all that limits how old a system can be.

The packages declare the libraries by hand, because no package manager can see them. The names
for every distribution are one table in `src/doctor.rs`, which `queijo --doctor` checks,
`packaging/dependencies.md` shows (a test keeps the file equal to it: `QUEIJO_UPDATE_DOCS=1 cargo
test dependencies_table` rewrites it) and a test compares with what the `.deb`, the `.rpm` and the
`PKGBUILD` declare. A new library or program goes in that table first.

What has actually been run: the x86_64 packages, installed on Debian 12, Ubuntu 22.04 and 24.04,
Fedora, Rocky Linux 8, openSUSE Tumbleweed and Arch Linux (CI's `distros` job; the same script runs
locally with docker). The aarch64 ones are cross-built the same way and nothing has run them.
Which desktops work is a separate question, answered in `docs/TESTING-DESKTOPS.md`.

## Building the packages locally

```sh
pip install ziglang                     # zig, the linker (or any zig on the PATH)
cargo install cargo-zigbuild cargo-deb cargo-generate-rpm --locked   # once
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu  # once
QUEIJO_ARCHES=x86_64 scripts/build-packages.sh   # writes dist/ (leave the variable out for both)
scripts/distro-test.sh debian:12                 # installs dist/'s package in a container and runs it
```

Handy checks, the same ones CI runs:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
QUEIJO_REQUIRE_FFMPEG=1 QUEIJO_REQUIRE_GSETTINGS=1 cargo test --locked
xvfb-run -a scripts/smoke-test.sh target/release/queijo
```

Without those two variables the tests that need `ffmpeg` or GNOME's settings
schemas are skipped instead of failing.
