# Acceptance and Testing

## Build Verification

| Command | Expected |
|---|---|
| `cargo build` | builds `sharpy` lib + bin (no GUI deps pulled in) |
| `cargo test` | 20/20 lib + integration tests pass (unaffected by GUI) |
| `cargo build -p sharpy-gui` | builds debug GUI; ≤ 30 s clean |
| `cargo build -p sharpy-gui --release` | release .exe produced |
| `cargo clippy -p sharpy-gui -- -D warnings` | zero warnings |
| `cargo clippy --workspace -- -D warnings` | zero warnings |

## Smoke Tests (Automated)

Two `#[test]`s in `sharpy-gui/src/state.rs`:

**1. Each preset's pipeline runs without error.**
```rust
#[test]
fn each_preset_produces_valid_output() {
    let source = std::sync::Arc::new(image::RgbImage::new(50, 50));
    for preset in PresetKind::ALL {
        let params = PipelineParams::from_preset(preset);
        assert!(build_pipeline(source.clone(), &params).is_ok(),
                "preset {:?} failed", preset);
    }
}
```

**2. GUI presets stay in sync with library presets (drift detection).**
For each preset, build the operations list two ways — the GUI's local
definition and `SharpeningPresets::*().operations()` — and compare. The
`Edge-Aware` case is allowed to differ in *order* but not in operation
parameters; the test checks the multiset of operations.

```rust
#[test]
fn gui_presets_match_library_presets() {
    use sharpy::SharpeningPresets;
    let dummy = sharpy::Image::from_rgb(image::RgbImage::new(1, 1)).unwrap();
    for preset in PresetKind::ALL {
        let lib_ops = match preset {
            PresetKind::Subtle    => SharpeningPresets::subtle(dummy.clone()),
            PresetKind::Moderate  => SharpeningPresets::moderate(dummy.clone()),
            PresetKind::Strong    => SharpeningPresets::strong(dummy.clone()),
            PresetKind::EdgeAware => SharpeningPresets::edge_aware(dummy.clone()),
            PresetKind::Portrait  => SharpeningPresets::portrait(dummy.clone()),
            PresetKind::Landscape => SharpeningPresets::landscape(dummy.clone()),
        }.operations().to_vec();
        let gui_ops = preset.expected_operations();
        // Order-independent comparison (Edge-Aware diverges in order):
        let mut a = lib_ops.clone(); let mut b = gui_ops.clone();
        a.sort_by_key(|op| op.name());
        b.sort_by_key(|op| op.name());
        assert_eq!(a, b, "preset {:?} drift between GUI and library", preset);
    }
}
```

Requires a one-time additive lib change: expose
`pub fn SharpeningBuilder::operations(&self) -> &[Operation]`.

These do not test UI rendering.

## Manual Test Checklist

Run each, mark pass/fail. The full list runs in <5 minutes.

### Loading
- [ ] Drag a JPEG onto the window — image displays.
- [ ] Drag a PNG with alpha — displays (alpha gets flattened, expected).
- [ ] "Open…" button opens native Windows file dialog.
- [ ] Dropping a .txt shows "Could not decode" in status bar; doesn't crash.
- [ ] Dropping a folder is ignored or shows a friendly message.
- [ ] Dropping a 50 MP image loads (preview is downscaled).
- [ ] Hovering a file over the window before dropping shows a visual hint.

### Sliders
- [ ] Dragging Unsharp Radius from 0.1 to 10 visibly changes the preview.
- [ ] Dragging Unsharp Amount from 0 to 5 visibly changes the preview.
- [ ] Threshold slider has integer increments and ranges 0–255.
- [ ] Toggling High-Pass on/off updates preview.
- [ ] Toggling Edges on/off and switching Sobel/Prewitt updates preview.
- [ ] Toggling Clarity on/off updates preview.
- [ ] Slider numeric labels match slider thumb positions.
- [ ] Slider drag on a 24 MP source feels smooth (no UI freezes >100 ms).

### Presets
- [ ] Each of the 6 presets applies and produces a distinct preview.
- [ ] Selecting the same preset twice is a no-op (no flicker).

### Reset
- [ ] "Reset All" returns sliders to defaults; preview matches a fresh load.
- [ ] Per-stage ↺ resets only that stage.

### Save
- [ ] "Save As…" opens a Windows save dialog with sensible default name.
- [ ] Saving as JPG produces a file that opens in Windows Photos.
- [ ] Saving as PNG produces a lossless file with same dimensions as source.
- [ ] Status bar shows save duration.
- [ ] Save with all stages disabled produces an unmodified copy.

### Edge cases
- [ ] Resizing the window: preview pane scales the image down to fit; right
      panel keeps fixed width.
- [ ] Closing the window terminates cleanly (no dangling worker thread).
- [ ] Loading a new image while a preview is mid-compute shows the new
      preview within ~1 second.

## Performance Targets

Measured on a recent desktop (8c/16t):

- Preview compute (768×768, all 4 stages, default params): ≤ 80 ms median.
- Preview compute (768×768, all 4 stages, **clarity radius = 20**): up to
  ~100 ms — at the edge of the budget; document, don't fail. Adaptive
  preview downscaling could mitigate but is out of scope.
- Save (4032×3024, all 4 stages, defaults): ≤ 4 s.
- Save (4032×3024, all 4 stages, **clarity radius = 20**): up to ~2 min.
  Acceptable because save runs on the worker, UI stays responsive.
- Cold launch (debug): ≤ 2 s. Release: ≤ 300 ms.
- Memory: full-res 24 MP source = ~270 MB; with up to 4 in-flight scratch
  buffers from the sharpening pipeline, peak ~600 MB. Document; don't
  treat as a fail.

If any of these are missed by >2× we should investigate before declaring
done.

## What "Ready for User Testing" Means

1. All build commands above pass.
2. The smoke test passes.
3. Every manual checklist item is verified by me, or explicitly noted as
   "couldn't test in this environment" with reason.
4. The release `.exe` runs without printing warnings/errors to stderr at
   startup with no image loaded.
5. The user gets an explicit "ready" message including:
   - The exact run command.
   - Path to the release binary.
   - List of any items I couldn't verify (so they know what to double-check).

## Known Limitations to Document

These will be listed in `sharpy-gui/README.md`:

- No support for HDR / 16-bit images (sharpy is u8 only).
- ICC profiles are stripped (the `image` crate doesn't preserve them).
- Dropping multiple files takes only the first.
- Save can take up to ~2 min for very large images at maximum clarity
  radius. Save runs on a worker thread so the UI stays responsive, and
  status bar shows "Saving…" while it's in flight.
- "Edge-Aware" preset uses the GUI's fixed pipeline order
  (`unsharp → high-pass → edges → clarity`), which is the **reverse**
  of `SharpeningPresets::edge_aware`'s native order. The GUI loads the
  preset's parameter values; the resulting image is therefore not
  byte-identical to running the same preset via the library or CLI.
  All other presets match the library's output exactly.
- No undo/redo.
- Not tested on macOS or Linux. eframe supports both, so it may work,
  but cross-platform is not a goal.

## Distribution

To hand a `.exe` to a non-Rust user:

1. `cargo build -p sharpy-gui --release` from the repo root.
2. The binary is at `target/release/sharpy-gui.exe`.
3. The MSVC Rust toolchain links the C runtime statically by default,
   so the `.exe` is portable — no VC++ Redistributable required on the
   target machine. No installer; no registry footprint.
4. Optional: `strip target/release/sharpy-gui.exe` (or set
   `[profile.release] strip = "symbols"` in workspace Cargo.toml) to
   shrink the binary.
