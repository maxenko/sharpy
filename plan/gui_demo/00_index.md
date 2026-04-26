# Sharpy GUI Demo — Plan Index

A simple, optional desktop demo app for Windows that loads an image (drag-drop or
file dialog) and lets the user explore every parameter of the four `sharpy`
sharpening algorithms with live sliders.

This is **a demo of library capability only**. It is not the canonical CLI, not a
production photo editor, and not a long-term-maintained product.

## Goals

1. Showcase every public sharpening parameter exposed by the `sharpy` library.
2. Provide live, interactive feedback as sliders move.
3. Be a drop-in `cargo run -p sharpy-gui` experience — no setup, no JS runtime,
   single Windows executable.
4. Stay small (~300–500 LOC of Rust). Not a feature factory.

## Non-Goals

- Not cross-platform tested. May happen to work on macOS/Linux because eframe is
  cross-platform, but we don't promise it.
- Not a replacement for the `sharpy` CLI.
- No undo history, no layers, no metadata editing, no export presets.
- No localization, no accessibility audit.
- Not published to crates.io — internal demo only.

## Plan Files

| File | What it covers |
|------|----------------|
| [01_architecture.md](01_architecture.md) | Workspace layout, crate boundaries, framework choice, dependency list |
| [02_ui_and_state.md](02_ui_and_state.md) | Window layout, state struct, parameter mapping, presets dropdown |
| [03_preview_pipeline.md](03_preview_pipeline.md) | Live preview strategy: downscaling, debouncing, off-UI-thread compute |
| [04_io_dragdrop.md](04_io_dragdrop.md) | Drag-drop, file dialog, save flow, supported formats |
| [05_implementation_phases.md](05_implementation_phases.md) | Vertical-slice phases with explicit "done" criteria |
| [06_acceptance_testing.md](06_acceptance_testing.md) | Manual test checklist, smoke tests, build verification |

## High-Level Decisions (locked)

- **Framework:** [`eframe`](https://crates.io/crates/eframe) (egui) — pure Rust,
  single .exe, drag-drop and sliders built in.
- **Layout:** Root crate becomes both a package and a workspace; new
  `sharpy-gui/` member sits alongside.
- **Optional:** GUI is a separate workspace member, so `cargo build` at the root
  for the lib does not pull in eframe and friends. Users opt in via `cargo
  build -p sharpy-gui` or `cargo run -p sharpy-gui`.
- **No public-API changes to `sharpy`** — the GUI consumes the existing public
  interface as any external user would.

## Out-of-Scope but Worth Noting

- Native Win32 widget look (would require `windows-rs`/`fltk-rs` and 3–5× the
  code). egui's renderer-drawn widgets ship as a single .exe and feel
  responsive — adequate for a demo.
- Pipeline reordering UI (drag stages around). Fixed order matches
  `SharpeningBuilder` defaults: unsharp → high-pass → edge-enhance → clarity.
- Histogram display, EXIF, color management, ICC profiles.
