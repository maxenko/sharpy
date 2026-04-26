# I/O: Drag-Drop, File Dialog, Save

## Drag-Drop (Open)

eframe surfaces dropped files via `egui::Context::input`:

```rust
ctx.input(|i| {
    if let Some(file) = i.raw.dropped_files.first() {
        if let Some(path) = &file.path {
            try_load_image(state, path);
        } else if let Some(bytes) = &file.bytes {
            // Web/wasm path — N/A for our Windows binary.
        }
    }
});
```

Hover-state visual feedback: when `i.raw.hovered_files` is non-empty, show a
dashed border and "Drop to open" overlay on the preview pane.

On Windows, dropped files arrive as full paths to local files — no special
handling needed.

## File Dialog (Open)

Toolbar button "Open…" calls:

```rust
let path = rfd::FileDialog::new()
    .add_filter("Image", &["jpg", "jpeg", "png", "bmp", "tiff", "webp"])
    .set_title("Open image")
    .pick_file();
if let Some(path) = path { try_load_image(state, &path); }
```

`rfd` on Windows wraps `IFileDialog` (the modern replacement for
`GetOpenFileName`). It returns control to the UI when the user picks or
cancels. The dialog is modal but does not block the worker thread.

## Image Decoding

Single shared loader:

```rust
fn try_load_image(state: &mut AppState, path: &Path) -> Result<()> {
    let dynamic = image::open(path).context("could not decode image")?;
    let rgb = dynamic.to_rgb8();
    let (w, h) = rgb.dimensions();
    let preview = downscale_to(PREVIEW_MAX_DIM, &rgb);

    state.source = Some(Arc::new(rgb));
    state.preview_source = Some(Arc::new(preview));
    state.source_path = Some(path.to_path_buf());
    state.dirty = true;
    Ok(())
}
```

Errors are surfaced in the status bar, not as modal dialogs. Failures we
expect:

- File not an image → "Could not decode <path>".
- Path doesn't exist (rare with drag-drop, possible with stale dialogs) →
  "File not found".
- Image exceeds `sharpy`'s 100 MP / 65536 dim limits — caught at
  `Image::from_rgb` time on the save path. We allow loading at preview
  resolution but warn at save: "Source exceeds sharpy's size limits". This is
  a reasonable demo behavior — preview still works.

## Supported Formats

Whatever the `image` crate supports with default features: JPEG, PNG, BMP,
TIFF, WebP. We do not enable GIF (animated source ambiguous), HDR (16-bit
HDR not handled by sharpy's u8 pipeline), or RAW (no DCRAW dependency).

## Save As…

```rust
let default_name = state.source_path.as_ref()
    .and_then(|p| p.file_stem())
    .map(|s| format!("{}_sharp.jpg", s.to_string_lossy()))
    .unwrap_or_else(|| "sharpened.jpg".into());

let save_path = rfd::FileDialog::new()
    .add_filter("JPEG", &["jpg", "jpeg"])
    .add_filter("PNG",  &["png"])
    .add_filter("TIFF", &["tif", "tiff"])
    .set_file_name(&default_name)
    .set_title("Save sharpened image")
    .save_file();
```

After dialog returns:

1. Build a `sharpy::Image` from the **full-resolution** source Arc.
2. Run `build_pipeline` with current params.
3. Call `image.save(path)` — the `image` crate picks the encoder by file
   extension.
4. Update status bar: "Saved → C:\…\photo_sharp.jpg (1.2 s)".

Overwrite confirmation: rfd handles this natively on Windows (the system
"Replace?" dialog from `IFileDialog`). We don't add our own.

## Drag-Drop Edge Cases

- **Multiple files dropped:** take the first, ignore the rest. Add a status
  message: "Loaded photo.jpg (2 other files ignored)".
- **Folder dropped:** ignore (or surface "Folders not supported").
- **Dropping while a save is in flight:** save now runs on the worker
  thread (no modal), so drag-drop is not blocked by it. The drop
  handler checks `state.save_in_flight` — if true, surface "Save in
  progress; please wait" in the status bar (info, 5s) and ignore the
  drop. Implementation: gate at the top of `try_load_image` and inside
  the file-dialog button handler.

## Status Bar Messages

Two message slots on `AppState`, one for **info** (transient, 5 s expiry)
and one for **errors** (sticky until the next user action that
supersedes them):

- Info (auto-expires): `Loaded photo.jpg (3024×4032) — preview 576×768`,
  `Saved photo_sharp.jpg (1.2 s)`.
- Error (sticky): `Could not decode foo.txt`,
  `Source exceeds size limits — save will fail at full resolution`.

Errors stay visible because a 5-second auto-expiry on a save failure
("permission denied", "disk full") is the kind of UX bug that makes users
miss why a save didn't happen.

**Per-category error clearing rules** (must be explicit, otherwise the UI
becomes magical):

| Error category | Cleared by |
|---|---|
| Load error (decode failure, file not found) | The next *successful* load. |
| Save error (write failure, size limit) | The next *successful* save. |
| Workspace/setup error (rare) | An explicit dismiss "✕" affordance. |

Different-category actions do **not** clear each other's errors: a
successful load does not clear a save error (and vice-versa). This keeps
each error visible until the user has actually fixed it.

Implementation:

```rust
struct StatusBar {
    info: Option<(String, Instant)>,                // 5s expiry
    error: Option<(StatusErrorCategory, String)>,   // sticky, per-category clearing
}

enum StatusErrorCategory { Load, Save }
```

## File Naming Convention

Default save filename appends `_sharp` before the extension. Matches the CLI's
default suffix in `Commands::Batch`.

## What Is Out Of Scope

- Watch-folder mode.
- Recent-files menu.
- Drag-out (dragging the result onto Explorer).
- ICC profile preservation. The `image` crate strips most metadata; the demo
  does not try to round-trip it.
- Async save with progress bar.
