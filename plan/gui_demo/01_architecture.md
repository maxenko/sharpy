# Architecture

## Workspace Layout

Convert the existing single-crate repo to a Cargo workspace whose root is also
the `sharpy` library package. Add `sharpy-gui` as a sibling workspace member.
The library's existing dependency tree is unaffected.

```
sharpy/
├── Cargo.toml         ← becomes both [package] for sharpy AND [workspace]
├── Cargo.lock         ← shared across the workspace
├── src/               ← unchanged: library + bin/sharpy
├── benches/
├── tests/
├── examples/
└── sharpy-gui/        ← new
    ├── Cargo.toml     ← [package] sharpy-gui, depends on sharpy = { path = ".." }
    └── src/
        ├── main.rs    ← App entry, eframe::run_native
        ├── app.rs     ← App struct + update loop
        ├── state.rs   ← Pipeline params, defaults, presets
        ├── preview.rs ← Downscale + worker-thread sharpen orchestration
        └── ui.rs      ← Side panel widgets, drag-drop handling, save flow
```

The root `Cargo.toml` gets a new `[workspace]` table:

```toml
[workspace]
members = ["sharpy-gui"]      # root package is implicitly a member
default-members = ["."]       # cargo build/test at root stays lib-only
resolver = "2"
```

Cargo permits a directory to be both a workspace root and a package. The
`default-members = ["."]` is critical: without it, `cargo build` and `cargo
test` at the root would build the entire workspace (pulling in eframe and
friends). With it, the existing lib/bin/test workflow is preserved exactly,
and the GUI is only built on explicit opt-in via `-p sharpy-gui` or
`--workspace`.

Listing `.` in `members` is redundant (the root package containing
`[workspace]` is implicitly a member) and idiom is to omit it.

### Cargo.lock

The repo's `.gitignore` currently excludes `Cargo.lock`. Library-only crates
typically gitignore the lock; binaries should track it for reproducible
builds. Once we ship a GUI binary, **remove `Cargo.lock` from `.gitignore`**
and commit the lock. This is a one-line change that lands as part of
Phase 0.

## Why a Workspace Member, Not a Feature Flag

Considered alternatives:

- **`[features] gui = ["eframe"]` + a feature-gated `[[bin]]`** — drags ~200
  transitive deps into the library's `Cargo.toml`. Anyone running
  `cargo metadata` on `sharpy` sees the GUI's deps even if they never enable
  the feature. Workspace members keep the dep graph cleanly partitioned.
- **Standalone repo** — overkill for a 300-LOC demo, and forces a published
  `sharpy` crate version to point at.
- **An `examples/gui.rs`** — examples should be small and didactic; they share
  the library's dev-deps. A real GUI binary needs its own build profile and
  resources.

Workspace member wins: clean separation, simple `cargo run -p sharpy-gui`,
shared `Cargo.lock` for deterministic builds.

## Framework Choice — eframe (egui)

| Need | eframe support |
|---|---|
| Sliders | `ui.add(egui::Slider::new(&mut value, range).text("label"))` |
| Drag-drop | `ctx.input(\|i\| &i.raw.dropped_files)` — built in |
| Image display | `egui::ColorImage` + `ui.image(texture)` |
| Single Windows .exe | `cargo build --release` produces one binary |
| Native menus | Not native, but egui's UI is fine for a demo |
| Async work | Standalone `std::thread` + channels; egui repaints on signal |

**Tradeoff accepted:** egui draws its own widgets, so the look does not match
the Windows 11 system theme. For a demo that exists to expose sharpen
parameters, this is fine.

**Rejected alternatives:**
- `iced` — comparable feature set, more boilerplate per widget.
- `slint` — declarative DSL adds learning curve.
- `fltk-rs` — closer to native look but uglier API.
- `tauri`/`wry` — pulls a JS toolchain. Violates the "simple" requirement.
- `windows-rs` / direct Win32 — true native, but ~10× the code.

## Dependencies

`sharpy-gui/Cargo.toml`:

```toml
[package]
name = "sharpy-gui"
version = "0.2.0"
edition = "2021"
rust-version = "1.76"           # eframe transitive minimum; verify at impl time
publish = false                 # internal demo, never published

[dependencies]
sharpy      = { path = ".." }
eframe      = { version = "*", default-features = false, features = ["default_fonts", "glow"] }
egui_extras = { version = "*", features = ["all_loaders"] }   # version must match eframe
image       = "0.25"            # same major as sharpy
rfd         = "0.15"            # native file open/save dialogs (Win32 IFileDialog)
anyhow      = "1.0"
log         = "0.4"
env_logger  = "0.11"
```

> **Version pinning at implementation time:** today is April 2026; `eframe`
> shipped 0.31 in early 2025 and the ecosystem has moved on. Phase 0 will
> resolve the actual current minor with `cargo add eframe` and pin both
> `eframe` and `egui_extras` to the same minor (egui crates release in
> lockstep). Treat the `*` above as a placeholder — the pinned version
> goes in once the implementation runs.

Notes:

- `default-features = false` on eframe drops `accesskit`, `wayland`, and
  `x11` integrations we don't need on a Windows-only target. Re-enable only
  `default_fonts` (so we get a usable font without bundling one) and `glow`
  (the OpenGL backend; alternative is `wgpu` which adds significant
  compile time).
- `egui_extras` with `all_loaders` lets egui decode JPEG/PNG/etc. directly into
  textures using the `image` crate. We may reduce this to specific loaders if
  compile time is a concern.
- `rfd` produces native file dialogs on Windows via `IFileDialog` (the COM
  replacement for `GetOpenFileName`).

Total expected new deps: eframe + winit + glow + egui crates ≈ 100 transitive
crates, but none of them touch the `sharpy` library's compile graph because
of `default-members = ["."]` at the workspace root.

## Versioning

`sharpy-gui` starts at `0.2.0` to match the library version it depends on. The
`publish = false` flag prevents accidental publishes.

## Build Targets

- `cargo build -p sharpy-gui` — debug build, ~15s clean compile.
- `cargo build -p sharpy-gui --release` — release build, single .exe in
  `target/release/sharpy-gui.exe`. Stripped of debug symbols by default in
  Cargo's release profile.
- The library's existing `cargo build` / `cargo test` are unaffected — they
  resolve only the lib + bin in the root crate.

## What This Architecture Explicitly Does NOT Do

- It does not modify the public API of the `sharpy` library. The GUI is a pure
  consumer.
- It does not add a runtime dependency on the GUI binary for any other code
  path.
- It does not introduce async runtimes (no `tokio`, no `async-std`). The
  preview worker is a single OS thread.
- It does not set up CI. The repo has no `.github/` workflows today; that
  remains true. A non-CI demo is acceptable for an internal showcase.

## Distribution

For a non-Rust user to test, build with `cargo build -p sharpy-gui --release`,
then ship `target/release/sharpy-gui.exe`. The MSVC Rust toolchain produces
binaries with a static C runtime linked, so no VC++ redistributable needs to
be installed on the target machine. The binary is portable — no installer,
no registry footprint.

## CLI vs GUI Binary Naming

After workspace conversion, `target/release/` will contain both:
- `sharpy.exe` — the CLI from the root crate's `[[bin]]`
- `sharpy-gui.exe` — the GUI from the `sharpy-gui` member

`cargo run` at the root runs the CLI (still the "default" binary).
`cargo run -p sharpy-gui` runs the GUI. The README will state this
unambiguously to avoid confusion.
