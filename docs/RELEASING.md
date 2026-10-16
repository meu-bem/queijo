# Branches, versions and releases

## Branches

| Branch | For | CI |
| --- | --- | --- |
| `test` | day-to-day work | formatting, lints, tests, smoke test on a virtual display, extension and launcher checks, and a build of the packages (kept as a workflow artifact for 14 days) |
| `main` | what has been released | the same checks, then the release is published |

Work on `test`. When it is ready to be released, open a pull request from `test` to
`main`; the checks run on the pull request too. Nothing else should be pushed to
`main`.

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
- creates the tag `v<version>` on that commit,
- publishes the release with the notes from `CHANGELOG.md` and these files:

| File | What it is |
| --- | --- |
| `queijo_<version>-1_amd64.deb` | package for Ubuntu 22.04 (installs the extension too) |
| `queijo-<version>-x86_64-linux.tar.gz` | the binary, launchers, icons and extension with `install.sh` |
| `queijo-gnome-extension-<version>.zip` | the GNOME Shell extension alone |
| `SHA256SUMS` | checksums of the three files above |

If the version was already released, the workflow ends with a notice and publishes
nothing: merging documentation or fixes to `main` without a version bump is safe.

## Supported environments

Packages are built, and tested, only for what has been validated: **Ubuntu 22.04
(GNOME 42, Wayland), x86_64**. The workflows run on `ubuntu-22.04` so the binary
doesn't need a newer C library. When another environment is validated, add its
package to `scripts/build-packages.sh` and the table above, and say so in the
README's Compatibility section.

## Building the packages locally

```sh
cargo install cargo-deb --locked   # once
scripts/build-packages.sh          # writes dist/
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
