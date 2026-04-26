# Ignored Plan Items

This file records plan items that subsequent `/plan-execute` runs should
treat as already handled or not applicable. Items marked `Already resolved`
are still re-verified every run; items marked `Not applicable` are skipped
permanently.

## Workspace/setup error has dismiss "✕" affordance

- **Plan source**: `04_io_dragdrop.md`, "Per-category error clearing rules" table
- **Status**: Not applicable
- **Reason**: The plan's table entry suggested workspace/setup errors would
  need an explicit "✕" dismiss affordance, separate from the per-category
  auto-clear logic for Load and Save. The implementation now has a `Worker`
  error category for app-fatal errors (worker thread gone, channel
  disconnected). Those errors are *intentionally* sticky with no dismiss
  button: when a Worker error fires, the only correct user action is to
  restart the app — adding a dismiss button would let the user hide a
  message that signals the app cannot do its job. No "✕" affordance is
  needed.

## Preset selector: dropdown rather than row of buttons

- **Plan source**: `02_ui_and_state.md`, "Presets Dropdown" + wireframe
- **Status**: Already resolved
- **Reason**: The plan explicitly called for a dropdown (`Preset: [ Subtle ▾ ]`).
  An earlier implementation used six buttons; this run replaced them with a
  proper `egui::ComboBox` in `ui.rs::draw_top_bar` that shows the active
  preset's name (or `(custom)` when params have been hand-tuned). Per-item
  hover tooltips for `Edge-Aware` are preserved.

## Worker-job panic categorization

- **Plan source**: `03_preview_pipeline.md`, "Worker Thread Lifecycle" + the
  Phase 5 scrutiny direction
- **Status**: Already resolved
- **Reason**: An earlier `catch_unwind` wrapper unconditionally returned
  `WorkerResult::SaveErr` regardless of which job kind panicked, miscategorizing
  preview-pipeline panics as save errors. `preview.rs` now snapshots a
  `JobKind` discriminator before the job is moved into `catch_unwind` and
  routes panics to the matching error variant (`PreviewErr` for preview jobs,
  `SaveErr` for save jobs). Re-verify on future runs.
