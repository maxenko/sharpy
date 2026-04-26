# sharpy-gui

Optional Windows desktop demo for the [`sharpy`](../) image-sharpening
library. Drag-drop an image, slide every parameter live, save the result.

This crate is **not published** and exists only to showcase the library.

## Run

```bash
cargo run -p sharpy-gui                  # debug
cargo run -p sharpy-gui --release        # release
```

The release binary is at `target/release/sharpy-gui.exe`. It's portable —
the MSVC Rust toolchain links the C runtime statically, so no Visual C++
redistributable is required on the target machine.

## Usage

1. **Open an image** by dragging it onto the window or clicking *Open…*.
2. **Tweak sliders** in the right panel. The preview re-renders live on a
   downscaled (max 768 px) copy.
3. **Pick a preset** from the toolbar to load common settings.
4. **Save As…** runs the full-resolution pipeline on a worker thread and
   writes to disk. The UI stays responsive throughout.

## Pipeline order

The GUI runs stages in a fixed order: **unsharp → high-pass → edges → clarity**.

This matches `SharpeningPresets::landscape` and `::strong`, and is
order-irrelevant for `subtle`, `moderate`, and `portrait`.

**Edge-Aware preset (marked with a `*`):** the library's
`SharpeningPresets::edge_aware` runs `edge_enhance → unsharp_mask`, the
*reverse* of the GUI's order. Selecting Edge-Aware loads the same parameter
values, but the GUI executes them in `unsharp → edges` order, so the result
is not byte-identical to running the same preset via the library or CLI.
Hover the dropdown entry for an in-app reminder.

## Limits and known issues

- Input must be ≤ 65536 × 65536 and ≤ 100 megapixels (the library's hard
  caps). Larger files are rejected at load time with a clear error.
- HDR / 16-bit images are not supported (the library is u8-only).
- ICC profiles are stripped — the `image` crate does not preserve them.
- Drag-drop with multiple files takes only the first; the others are
  noted in the status bar.
- Save can take up to ~2 minutes for very large images at maximum clarity
  radius (the underlying algorithm is O(radius²) per pixel). Save runs on
  a worker thread, so the UI stays responsive while saving.
- Closing the window during a save waits for the save to finish before
  exiting. Up to ~2 minutes in pathological cases.
- Not tested on macOS or Linux. eframe supports both, so it may work, but
  cross-platform is not a goal.

## Architecture

Three modules under `src/`:

- `state.rs` — `AppState`, `PipelineParams`, `StageParams<T>`, presets.
- `preview.rs` — `WorkerJob` / `WorkerResult` channel types, `PreviewWorker`
  thread, `decode_and_prepare` (with pre-decode dimension validation),
  `build_pipeline`.
- `ui.rs` — egui drawing helpers (top bar, controls panel, central pane,
  status bar).
- `app.rs` — `App` struct + `eframe::App::ui` orchestration.

Two crossbeam-channels connect the UI to the worker:

- Preview channel: `bounded(1)`, coalesces rapid slider events.
- Save channel: `unbounded()`, never drops a save.

The worker uses `crossbeam_channel::select!` to pull from both, with
`catch_unwind` so a panic during a pipeline run doesn't kill the thread.
Source-image generation tracking ensures stale preview results from a
prior image are dropped after the user loads a new one.
