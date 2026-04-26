# Implementation Phases

Each phase is a vertical slice that produces a runnable binary. Don't move to
phase N+1 until phase N runs.

## Phase 0 — Workspace bootstrap

**What:** Convert root crate to also be a workspace; add `sharpy-gui/`
skeleton that compiles as an empty window.

**Steps:**

1. Edit root `Cargo.toml`: add
   ```toml
   [workspace]
   members = ["sharpy-gui"]
   default-members = ["."]
   resolver = "2"
   ```
   The `default-members = ["."]` is critical — without it, root-level
   `cargo build`/`cargo test` would build the entire workspace.
2. Remove `Cargo.lock` from `.gitignore` and commit it. Binaries should
   pin a lock for reproducible builds.
3. Create `sharpy-gui/Cargo.toml`. Resolve current eframe minor with
   `cargo add eframe`; pin `egui_extras` to the same minor. `publish =
   false`. Add `rust-version` matching whatever eframe transitively
   requires.
4. Create `sharpy-gui/src/main.rs`:
   ```rust
   fn main() -> eframe::Result<()> {
       env_logger::init();
       let options = eframe::NativeOptions {
           viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 700.0]),
           ..Default::default()
       };
       eframe::run_native(
           "Sharpy Demo",
           options,
           Box::new(|_cc| Ok(Box::new(App::default()) as Box<dyn eframe::App>)),
       )
   }

   #[derive(Default)]
   struct App;
   impl eframe::App for App {
       fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
           egui::CentralPanel::default().show(ctx, |ui| {
               ui.heading("Sharpy Demo");
           });
       }
   }
   ```
   The explicit `as Box<dyn eframe::App>` coercion is required: the closure
   returns `Result<Box<dyn eframe::App>>`, and `Box::<App>::default()`
   produces a concrete `Box<App>` that doesn't coerce automatically inside
   `Ok(...)`.
5. `cargo run -p sharpy-gui` — empty window with title appears.
6. `cargo test` (no `-p` or `--workspace`) — existing 20 lib/integration
   tests still pass and no GUI deps are pulled in (verifies
   `default-members` is wired correctly).

**Done when:** the binary runs and the lib's tests are unaffected.

## Phase 1 — Load + display

**What:** Drag-drop or Open… loads an image; display the source in the
center pane. No sliders yet.

**Steps:**

1. `state.rs` skeleton: `AppState { source, preview_source, source_path,
   last_message }`.
2. `try_load_image` helper as in [04_io_dragdrop.md].
3. UI: top toolbar with "Open…", central panel that shows the loaded image
   (or "Drag an image here" placeholder).
4. Drag-drop handler reading `ctx.input(|i| i.raw.dropped_files)`.
5. Preview source downscale at load time (single call to
   `image::imageops::resize`).
6. Texture upload via `egui::ColorImage::from_rgb` and
   `ui.ctx().load_texture`. Cache the `TextureHandle` in `AppState` and
   re-upload only when the source changes — texture upload is non-trivial.
7. Status bar with source dimensions.

**Done when:** dragging a JPG onto the window displays it (downscaled to
fit), and the status bar shows correct dimensions.

## Phase 2 — Slider panel + synchronous preview

**What:** Right-side panel with all 4 stage groups; sliders rerun the full
pipeline on the **preview-resolution** source synchronously on the UI thread.
No worker thread yet.

**Steps:**

1. `state::PipelineParams` + `StageParams<T>` + `Default` impls per
   [02_ui_and_state.md].
2. `ui::draw_controls(ui, &mut state.params)` — emit sliders, checkboxes,
   radio buttons. Sliders use ranges/defaults from the UI table.
3. `preview::build_pipeline(image, &params)` per [02_ui_and_state.md].
4. After every frame: if `params` changed since last frame, recompute the
   preview output and re-upload the texture.
5. Status bar shows `last_run_ms`.

**Done when:** moving any slider visibly changes the preview image. Note
that this phase will be jerky for large source images — that's expected;
phase 3 fixes it.

## Phase 3 — Worker thread + coalescing

**What:** Move the preview compute off the UI thread; coalesce rapid slider
events.

**Pre-step (lib):** Add `pub fn SharpeningBuilder::operations(&self) -> &[Operation]`
to `src/builder.rs`. One-line additive accessor. Bump root version to
`0.2.1` to reflect the additive API change. Existing tests must still
pass; add one unit test for the new accessor.

**Steps:**

1. Add `crossbeam-channel = "0.5"` to `sharpy-gui/Cargo.toml`.
2. `preview.rs` defines `WorkerJob` and `WorkerResult` enums (see
   [03_preview_pipeline.md] "Channel Types"). Phase 3 only uses the
   `Preview` variants of each.
3. `App` owns `preview_tx: Option<crossbeam_channel::Sender<WorkerJob>>`,
   `save_tx: Option<crossbeam_channel::Sender<WorkerJob>>`,
   `results_rx: crossbeam_channel::Receiver<WorkerResult>`,
   `worker_handle: Option<thread::JoinHandle<()>>`. Both senders are
   `Option` so `Drop` can take them before joining.
4. Worker spawned in `App::new`. Uses `crossbeam_channel::bounded(1)` for
   preview, `unbounded()` for save, joined in `Drop` after senders are
   dropped.
5. Each frame: drain `results_rx`, update `preview_texture` via
   `TextureHandle::set` (not `load_texture`), and if `state.pending` is
   `Some`, retry sending and call
   `ctx.request_repaint_after(Duration::from_millis(16))` (skip the
   repaint hint when `save_in_flight` to avoid 60 Hz spinning during a
   long save).
6. UI on parameter change: `preview_tx.try_send(WorkerJob::Preview { … })`.
   If full, stash params in `state.pending`.
7. Worker passes `egui::Context` clone to call `ctx.request_repaint()`
   when it produces a result, so egui wakes up to render it.

**Done when:** dragging a slider fast on a 24 MP source feels smooth; the
preview catches up within a few hundred ms after release; the new lib
accessor has its own unit test passing.

## Phase 4 — Save (on worker) + presets + reset + polish

**What:** Save As… that runs on the same worker thread (NOT on the UI
thread — clarity at radius=20 on a 24 MP source can take 30+ seconds);
preset dropdown; reset buttons; sticky-error / transient-info status
messages.

**Steps:**

1. Convert worker job type to an enum: `WorkerJob::Preview { source,
   params }` and `WorkerJob::Save { source, params, path }`. Worker
   serializes them; preview jobs queued behind a save run after save
   completes.
2. "Save As…" toolbar button → rfd save dialog (still on UI thread —
   it's a few-second operation max) → send `WorkerJob::Save` over the
   params channel. Disable the Save button while in flight.
3. Worker on `Save`: build full-res pipeline, `image.save(path)`, send
   back `WorkerResult::Saved { path, elapsed_ms }`. UI updates status
   bar and re-enables Save button.
4. Preset dropdown — six presets. On select, replace `state.params` and
   mark dirty. Document the `edge_aware` order mismatch in the GUI
   README.
5. Reset All button + per-stage ↺ buttons (`ui.small_button("↺")`).
6. Status bar: split info (5s expiry) from errors (sticky until
   superseded).
7. Hovered-files visual feedback (dashed border + "Drop to open"
   overlay).
8. App icon (optional, can skip).

**Done when:** all UX described in [02_ui_and_state.md] works; saving
a full-res image with all stages enabled and clarity at radius=20 does
NOT freeze the UI; saved output is correct.

## Phase 5 — Scrutiny + refactor

Spawn parallel review agents; address findings; final cleanup pass.

**Done when:**

- `cargo clippy -p sharpy-gui -- -D warnings` is clean.
- `cargo build -p sharpy-gui --release` produces a working binary.
- Manual test checklist [06] passes.
- LOC budget: aim for < 500 lines of Rust in `sharpy-gui/src/`.

## Sequencing

Phases must run in order. Phase 1 can produce a usable demo if needed — it
displays images. Phase 2 produces a sharpening demo (just slow). Phase 3
makes it feel professional. Phase 4 makes it complete.

If we stop at any phase the codebase is still in a coherent shippable
state — no half-finished features.
