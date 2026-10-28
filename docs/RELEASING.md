# Releasing

Releases are tag-driven. Pushing a `0.*` tag builds every platform, runs the
engine suite and publishes the binaries with the matching changelog section.

## Versioning

The workspace version lives in the root `Cargo.toml`:

```toml
[workspace.package]
version = "0.0.2"
```

Both crates inherit it (`version.workspace = true`), so there is exactly one place
to bump. The tag must match: `0.0.1`, `0.0.2`, `0.1.0`, …

`0.0.x` is the pre-release line: the engine and the desktop shell are complete,
the plug-in builds do not exist yet. `0.1.0` is reserved for the first release
where the audio path is considered stable across all three platforms.

## Steps

1. Update `CHANGELOG.md` with the new section, newest first.
2. Bump `version` in the root `Cargo.toml`.
3. Refresh the screenshots if the UI changed:

   ```bash
   REPLICANTSYNTH_SCREENSHOT=screenshots/grid_playing.png \
   REPLICANTSYNTH_SCREENSHOT_FRAMES=200 \
   REPLICANTSYNTH_SCREENSHOT_SCENE=0 \
   cargo run --release -p replicantsynth-desktop
   ```

4. Verify locally:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --release --workspace
   cargo build --release -p replicantsynth-desktop
   ```

5. Commit, tag, push:

   ```bash
   git add -A
   git commit -m "release: 0.0.1"
   git tag -a 0.0.1 -m "ReplicantSynth Desktop 0.0.1"
   git push origin main
   git push origin 0.0.1
   ```

## What the workflow does

`.github/workflows/desktop.yml`:

- **check** — `cargo fmt --all --check`, `cargo clippy --workspace --all-targets
  -- -D warnings` on Linux.
- **test** — `cargo test --release --workspace`, then an offline demo render on
  Linux, macOS and Windows, uploading the rendered WAV as an artifact.
- **build** — release binaries per platform (`replicantsynth`, plus the
  `render-demo` tool), built on `ubuntu-24.04` (glibc 2.39+), `macos-14`
  (Apple silicon) and `windows-2022`.
- **ui** — runs the app under `xvfb-run`, captures a frame and checks the PNG.
  It is advisory: software OpenGL availability varies between runners, so it
  never blocks a release.
- **release** — on a `0.*` tag, downloads the platform bundles and creates a
  GitHub release whose body is the changelog section for that version.

The binary names are stable, so download links in the README can point at
`releases/latest/download/replicantsynth-linux-x86_64.tar.gz` and friends.

## Local release build

```bash
cargo build --release -p replicantsynth-desktop -p replicantsynth-core
ls -lh target/release/replicantsynth target/release/render-demo
```

Strip symbols to shrink the binary:

```bash
strip target/release/replicantsynth
```

`[profile.release]` already enables `lto = "thin"`, `codegen-units = 1` and
`strip = "symbols"`.
