# Live Preview Pipeline

## Goals

- Slider drag feels responsive (≤80 ms perceived latency).
- Full-resolution image is processed only on Save (not while sliding).
- The UI thread never blocks on a sharpen call.
- Stale work is dropped when the user is still moving the slider.

## Strategy: downscaled preview + worker thread + coalescing channel

```
                     ┌────────────────────┐
   slider drag       │   UI thread        │
   ─────────────►    │   AppState mutated │
                     │   dirty = true     │
                     └─────────┬──────────┘
                               │ try_send(latest_params)   (mpsc, capacity 1)
                               ▼
                     ┌────────────────────┐
                     │  Preview worker    │ (single OS thread)
                     │  build_pipeline at │
                     │  preview-resolution│
                     └─────────┬──────────┘
                               │ send(preview_image_bytes)  (mpsc unbounded)
                               ▼
                     ┌────────────────────┐
                     │   UI thread        │
                     │   upload texture,  │
                     │   ctx.request_repaint()
                     └────────────────────┘
```

## Preview Resolution

`PREVIEW_MAX_DIM = 768` (longest side). The downscale is computed once at
load-time using `image::imageops::resize` with `FilterType::Triangle` and
cached as `Arc<RgbImage>`. All preview pipeline work runs against this Arc.

A 768×768 image through the full pipeline (all four stages) measures ~30–60 ms
on a modern desktop with rayon enabled — well under the 80 ms budget.

## Channel Types (defined once, used from Phase 3 onward)

```rust
pub enum WorkerJob {
    Preview { source: Arc<RgbImage>, params: PipelineParams },
    Save    { source: Arc<RgbImage>, params: PipelineParams, path: PathBuf },
}

pub enum WorkerResult {
    Preview { image: RgbImage, params: PipelineParams, elapsed_ms: u128 },
    Saved   { path: PathBuf, elapsed_ms: u128 },
    SaveErr { error: String },
}
```

The channel item type is `WorkerJob` from day one. Phase 3 ships only the
`Preview` variant; Phase 4 adds `Save` handling on the worker side without
changing the channel type. This avoids the type-mismatch trap of "rename
from PreviewJob to WorkerJob mid-implementation".

## Two Channels: Coalesced Preview + Direct Save

A single channel would let a Preview overwrite a pending Save, which we
must never allow. Use **two channels** with `crossbeam-channel` (added as
a dep — `std::sync::mpsc` lacks `select`):

- `preview_tx: crossbeam_channel::Sender<WorkerJob>` — capacity-1
  (`bounded(1)`), coalesces Preview jobs (latest-only).
- `save_tx: crossbeam_channel::Sender<WorkerJob>` — `unbounded()`; Save
  is rare and must never be dropped.

Worker loop:

```rust
use crossbeam_channel::select;
loop {
    select! {
        recv(save_rx) -> msg => match msg {
            Ok(job) => run_save(job),
            Err(_) => break, // app shutting down
        },
        recv(preview_rx) -> msg => match msg {
            Ok(job) => run_preview(job),
            Err(_) => break,
        },
    }
}
```

`select!` picks one available job at random when both are ready; that's
fine — saves are rare so the chance of starving them is negligible. If
needed we can use `select_biased!` to prefer save.

Add to `sharpy-gui/Cargo.toml`:

```toml
crossbeam-channel = "0.5"
```

## Coalescing Channel (preview path)

Behavior:

- UI calls `try_send(params)`. If the buffer is empty, the params land. If a
  prior un-consumed item is sitting there, the UI **drops the new send** and
  remembers the params in `AppState.pending: Option<PipelineParams>`.
- When `pending` is `Some`, the UI also calls `ctx.request_repaint_after(
  Duration::from_millis(16))` so egui guarantees another tick even if the
  user has stopped moving the slider. This closes the "stale-by-one-frame"
  gap: without the repaint hint, if the user releases the slider exactly
  when the channel is full, `pending` would sit until some other input
  fired.
- After the worker finishes a job, it loops back, drains the channel, and
  picks up the freshest params. The UI thread re-sends `pending` next frame.

This is the standard "latest-only" pattern. Capacity-1 mpsc is sufficient
because we have one producer and one consumer.

Concretely (Phase 3 / preview path):

```rust
// UI (every frame, after detecting params change), using crossbeam-channel:
let job = WorkerJob::Preview { source: source_arc.clone(), params };
match preview_tx.try_send(job) {
    Ok(()) => state.pending = None,
    Err(TrySendError::Full(_)) => {
        state.pending = Some(params);
        // Skip request_repaint_after if a save is in flight — the save's
        // own completion will trigger a repaint, and there's no point
        // ticking 60 Hz for 2 minutes.
        if !state.save_in_flight {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    Err(TrySendError::Disconnected(_)) => { /* worker died, log + give up */ }
}
```

## Worker Thread Lifecycle

- Spawned once in `App::new` with the params channel receiver and the result
  channel sender.
- Loop:
  1. `recv()` blocks until new params arrive (or channel hangs up).
  2. Coalesce: drain any further pending items via `try_recv()`, keep the last.
  3. Build a `sharpy::Image` from the cached `preview_source` Arc using
     `Image::from_arc_rgb` (clean construction site; copy-on-write happens
     at the first stage's `get_mut`).
  4. Run `build_pipeline`. Time the call.
  5. Send `WorkerResult::Preview { image, params, elapsed_ms }` back to the UI.
  6. Call `ctx.request_repaint()` via a stored `egui::Context` clone so egui
     wakes up to display the new texture.
- Stops when the params-channel sender is dropped (App shutting down — see
  shutdown section).

Why a hand-rolled thread instead of `rayon::spawn`? rayon's pool is shared
with the per-pixel parallelism inside `sharpy`. Pinning the orchestrator to
its own OS thread keeps the pipeline fully exploiting rayon for inner work.

## Worker Shutdown

`App` owns:

```rust
pub struct App {
    // ...other state...
    preview_tx: Option<crossbeam_channel::Sender<WorkerJob>>,
    save_tx:    Option<crossbeam_channel::Sender<WorkerJob>>,
    worker_handle: Option<thread::JoinHandle<()>>,
}
```

Both senders are `Option` precisely so `Drop` can take and drop them
*before* joining the handle. Without that ordering, the join would
deadlock (worker still has live receivers, blocks forever in `select!`).

```rust
impl Drop for App {
    fn drop(&mut self) {
        // Dropping the senders causes the receivers to disconnect; the
        // worker's select! arms will return Err on next iteration and
        // the loop breaks.
        self.preview_tx.take();
        self.save_tx.take();
        if let Some(handle) = self.worker_handle.take() {
            // Best-effort join; OS will reap if a Save is mid-flight
            // and takes longer than the user's patience.
            let _ = handle.join();
        }
    }
}
```

If a save is mid-compute when the user closes the window, the join will
block until save completes. For a demo this is acceptable (saves take
seconds to minutes; the window closes when save finishes). If we need
"close immediately and abandon save", add an `AtomicBool` cancel flag
the worker checks between stages — out of scope.

## Source Loading

When a new image loads:

1. Decode on the UI thread (it's a one-shot, ≤200 ms for typical photos and
   the user expects a tiny pause after dropping a file).
2. Compute the preview-resolution copy on the UI thread (one downscale
   operation, ≤30 ms for typical photos).
3. Store both as `Arc<RgbImage>` in `AppState`.
4. Mark dirty → preview worker picks up the new params + new source.

The preview worker holds a `Arc<RgbImage>` ref to whatever source was current
at job-start time, so a load-during-compute does not cause a use-after-free —
the old Arc lives until the in-flight job completes.

## Save (Full Resolution) — runs on worker thread

**Earlier draft estimated 1–4 s for 24 MP. That estimate was wrong.**
`sharpy::clarity` is O(window²) per pixel where window = `(radius * 2).round()`.
At max radius (20), that's a 40×40 = 1600-sample local-mean per pixel; on
24 MP × 16 cores rayon, expect **30 s – 2 min**, not 1–4 s. Freezing the UI
that long triggers Windows' "Not Responding" overlay and makes users force-kill.

So save runs on the worker thread, not the UI thread:

1. User clicks Save As… → `rfd` opens the native save dialog (modal). This
   one is fine on the UI thread — it returns within a few seconds.
2. After dialog returns, send a `WorkerJob::Save { source: Arc<RgbImage>,
   params, path }` over the params channel. Show "Saving…" in the status bar
   and disable the Save button.
3. The worker handles save and preview jobs from the same channel, so it
   serializes them naturally — no worker contention. (Concretely, the
   channel item type becomes an enum: `Preview { source, params }` |
   `Save { source, params, path }`.)
4. When save completes, the worker sends a `WorkerResult::Saved { path,
   elapsed_ms }` back; UI displays "Saved <path> (Xs)" and re-enables the
   Save button.

While save is in progress:
- Sliders remain interactive (they update params), but new preview jobs are
  queued behind the in-progress save. The user sees a brief lag between
  slider change and preview update — acceptable.
- A second "Save" click is ignored (button disabled).

This collapses preview and save into a single worker model with one ordered
queue, removing the previous concern about rayon contention between
preview-on-worker and save-on-UI-thread.

## What Triggers a Recompute

- Any field of `PipelineParams` changes (`PartialEq` differs from last-sent).
- Source image changes.
- Stage `enabled` toggles.

What does **not** trigger a recompute:

- Window resize.
- Switching presets to one identical to current params (PartialEq guards).
- Repaints driven solely by hover effects or texture re-upload.

## Error Handling

`build_pipeline` returns `Result<Image, ImageError>`. Possible errors:

- **Invalid parameter** — shouldn't happen because slider ranges are clamped
  to library-valid ranges. If it does (programmer error), surface in status
  bar with red text. Don't crash.
- **Image too large** — only relevant on Save with the full-res source. Show
  in status bar, keep the previous preview visible.

The preview path itself cannot fail in practice (preview source is always
small), but the result type is preserved for symmetry with Save.

## Profiling Hooks

The `last_run_ms` field is shown in the status bar to give users a feel for
parameter cost. Optionally include rayon thread count: `rayon::current_num_threads()`.

## Texture Upload Reuse

Preview results land in the UI as `RgbImage`. Naively each result calls
`ctx.load_texture` which allocates a new GPU texture. For 768×768×3 = ~1.7 MB
uploaded at up to 12 fps during a slider drag = ~20 MB/s. While that's
inside modern PCIe budgets, it's needless churn.

Use a single persistent `TextureHandle` cached on `AppState`:

```rust
state.preview_texture
    .get_or_insert_with(|| ctx.load_texture("preview", img_initial, opts))
    .set(ColorImage::from_rgb([w, h], &rgb_bytes), opts);
```

`TextureHandle::set` updates the existing GPU texture in place — much
cheaper than re-allocating. Reset the handle to `None` only when the source
image dimensions change (preview-source dimensions can change between
loads but not between slider moves).

## What This Pipeline Explicitly Does NOT Have

- No GPU compute path. CPU/rayon only.
- No tile-based incremental rendering.
- No partial preview (it's all or nothing).
- No undo. Resetting parameters reproduces the original by toggling
  everything off.
