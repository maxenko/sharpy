# UI Layout and State Model

## Window Layout

```
┌──────────────────────────────────────────────────────────────────────┐
│  [Open…]  [Save As…]  [Reset All]   Preset: [ Subtle ▾ ]  [Apply]   │
├────────────────────────────┬─────────────────────────────────────────┤
│                            │  ▣ Unsharp Mask                         │
│                            │     Radius     [────●────]  1.00        │
│                            │     Amount     [─●──────]  0.50         │
│                            │     Threshold  [●───────]   0           │
│                            │                                         │
│   PREVIEW IMAGE            │  ☐ High-Pass Sharpen                    │
│   (drag-drop here)         │     Strength   [──●─────]  0.50         │
│                            │                                         │
│   1024×768                 │  ☐ Edge Enhance                         │
│   downscaled to 768 max    │     Strength   [──●─────]  0.50         │
│                            │     Method     ( ● Sobel  ○ Prewitt )   │
│                            │                                         │
│                            │  ☐ Clarity                              │
│                            │     Strength   [──●─────]  0.50         │
│                            │     Radius     [──●─────]  3.0          │
├────────────────────────────┴─────────────────────────────────────────┤
│  Source: photo.jpg (3024×4032) • Preview: 576×768 • 38 ms last apply │
└──────────────────────────────────────────────────────────────────────┘
```

- **Top bar:** action buttons + preset dropdown.
- **Left pane:** the preview image, also the drag-drop target. When no image is
  loaded, displays a "Drag an image here, or click Open" hint.
- **Right pane:** sticky controls panel, ~340 px wide. Each algorithm group has
  an enable checkbox, parameter sliders, and shows current numeric value.
- **Bottom status bar:** source filename + dimensions, preview dimensions, last
  pipeline run wall-clock time.

The right pane uses `egui::SidePanel::right` so the user can resize. The
preview pane is a `egui::CentralPanel`. The status bar is a
`egui::TopBottomPanel::bottom`.

## State Struct

```rust
// sharpy-gui/src/state.rs

pub struct AppState {
    /// Loaded source at native resolution. None until an image is loaded.
    pub source: Option<Arc<RgbImage>>,
    /// Source path for status display + default save location.
    pub source_path: Option<PathBuf>,
    /// Downscaled source for live preview compute (max dim = PREVIEW_MAX).
    pub preview_source: Option<Arc<RgbImage>>,
    /// Most recent preview output, ready to be uploaded as a texture.
    pub preview_output: Option<RgbImage>,
    /// Last preview duration in ms, for status bar.
    pub last_run_ms: Option<u128>,
    /// All pipeline parameters.
    pub params: PipelineParams,
    /// Set when the user changed params and a new preview compute is queued.
    pub dirty: bool,
}

#[derive(Clone, PartialEq)]
pub struct PipelineParams {
    pub unsharp:   StageParams<UnsharpParams>,
    pub high_pass: StageParams<HighPassParams>,
    pub edges:     StageParams<EdgeParams>,
    pub clarity:   StageParams<ClarityParams>,
}

#[derive(Clone, PartialEq)]
pub struct StageParams<T> {
    pub enabled: bool,
    pub params: T,
}

#[derive(Clone, PartialEq)]
pub struct UnsharpParams { pub radius: f32, pub amount: f32, pub threshold: u8 }

#[derive(Clone, PartialEq)]
pub struct HighPassParams { pub strength: f32 }

#[derive(Clone, PartialEq)]
pub struct EdgeParams { pub strength: f32, pub method: EdgeMethod }

#[derive(Clone, PartialEq)]
pub struct ClarityParams { pub strength: f32, pub radius: f32 }
```

`PipelineParams: PartialEq` is the change-detection mechanism: the preview
worker only runs when the current params differ from the last-applied params.

## Slider Ranges and Defaults

Ranges match what `sharpy::Image` validates internally. Defaults are chosen to
produce a visible-but-not-extreme result on a typical photo.

| Stage      | Param      | Range          | Default | Slider type     |
|------------|------------|----------------|---------|-----------------|
| Unsharp    | radius     | 0.1 .. 10.0    | 1.0     | f32, log scale  |
| Unsharp    | amount     | 0.0 .. 5.0     | 0.5     | f32, linear     |
| Unsharp    | threshold  | 0 .. 255       | 0       | u8,  linear     |
| High-Pass  | strength   | 0.05 .. 3.0    | 0.5     | f32, linear     |
| Edges      | strength   | 0.05 .. 3.0    | 0.5     | f32, linear     |
| Edges      | method     | Sobel / Prewitt| Sobel   | radio buttons   |
| Clarity    | strength   | 0.05 .. 3.0    | 0.5     | f32, linear     |
| Clarity    | radius     | 0.5 .. 20.0    | 3.0     | f32, linear     |

The `library minimum exclusive 0.0` constraint is enforced by clamping slider
minimums slightly above zero (0.05/0.1/0.5) — the user cannot drag to invalid
input.

## First-Launch Defaults

The app boots with the **Moderate** preset already loaded. That means
Unsharp Mask + Clarity are both ON, with the values defined by
`SharpeningPresets::moderate`. Rationale: a "demo of capability" should
showcase pipeline composition (multiple stages combining), not just a single
unsharp mask the user has seen in every photo editor. The user can immediately
toggle stages off to see the effect of each one in isolation, or pick a
different preset.

## Presets Dropdown

The dropdown loads one of the six `SharpeningPresets` configurations:

```
Subtle  Moderate  Strong  Edge-Aware *  Portrait  Landscape
```

Selecting a preset:

1. Sets the corresponding `enabled` flags on each `StageParams`.
2. Replaces all parameter values with the preset's hard-coded numbers.
3. Marks state dirty → triggers preview recompute.

Preset values mirror exactly what `sharpy::SharpeningPresets::*` returns.
The GUI re-implements them locally (≤30 LOC) rather than introspecting the
library — keeps the lib's public API frozen.

**Drift protection:** the smoke test (see `06_acceptance_testing.md`)
constructs each preset both via the GUI's local definition AND via the
library's `SharpeningPresets::*`, then compares the resulting operations.
If lib presets ever change values, the GUI test fails loudly. This requires
a single 3-line additive change to the library: a public accessor
`SharpeningBuilder::operations(&self) -> &[Operation]` so the test can
read what the library produced. The accessor is non-mutating and adds zero
behavioral risk — it's exactly the same shape as similar accessors on
existing types.

**`Edge-Aware` UI cue:** the dropdown entry is rendered with a trailing
asterisk and a hover tooltip:

> "Edge-Aware: parameter values match `SharpeningPresets::edge_aware`,
> but the GUI executes stages in a fixed order (unsharp → high-pass →
> edges → clarity), which differs from the library preset's
> edges → unsharp ordering. Result will differ from CLI output."

This makes the limitation visible to users without forcing them to read
the README.

## Reset Buttons

- **Reset All** (top bar): restore factory defaults across every stage.
- Per-stage: a small "↺" icon next to each stage header that resets only that
  stage. Implemented via a shared `Default` impl on each `StageParams<T>`.

## Pipeline Order

Fixed: **unsharp → high-pass → edge-enhance → clarity**.

This order matches `SharpeningPresets::landscape` and `::strong` (and is
order-irrelevant for `subtle`, `moderate`, `portrait` since they only have
one or two stages whose order doesn't matter for the active set).

**Known mismatch — `edge_aware` preset:**
`SharpeningPresets::edge_aware` runs `edge_enhance → unsharp_mask`, the
*reverse* of our fixed GUI order. Selecting "Edge-Aware" in the GUI
preset dropdown loads the same parameter values, but the GUI executes
them in `unsharp → edges` order, so the resulting image will not be
byte-identical to `SharpeningPresets::edge_aware(img).apply()`. Since
both orderings are reasonable and the GUI is a demo (not a faithful
preset reproducer), we accept this and document it in the GUI's
README. Pipeline reordering UI is explicitly out of scope.

## Building the Pipeline

```rust
pub fn build_pipeline(source: Arc<RgbImage>, p: &PipelineParams) -> sharpy::Result<Image> {
    // The UI also keeps the Arc alive for the next frame, so try_unwrap
    // here always fails and we take the ImageData::Shared branch. The
    // first stage's get_mut() does a copy-on-write clone — same work as
    // ::from_rgb((*arc).clone()) but allocated lazily, and the cleanest
    // way to construct an Image from an Arc without an explicit clone
    // call site.
    let image = Image::from_arc_rgb(source)?;
    let mut b = image.sharpen();
    if p.unsharp.enabled {
        let u = &p.unsharp.params;
        b = b.unsharp_mask(u.radius, u.amount, u.threshold);
    }
    if p.high_pass.enabled {
        b = b.high_pass(p.high_pass.params.strength);
    }
    if p.edges.enabled {
        let e = &p.edges.params;
        b = b.edge_enhance(e.strength, e.method);
    }
    if p.clarity.enabled {
        let c = &p.clarity.params;
        b = b.clarity(c.strength, c.radius);
    }
    b.apply()
}
```

Single source of truth used by both preview and save. Use
`Image::from_arc_rgb` for the cleanest construction site; the actual
clone cost is identical to `Image::from_rgb((*arc).clone())` because
the UI retains an Arc reference, but the deferred copy-on-write keeps
peak memory lower and the API call site cleaner.

## Note on `EdgeMethod: PartialEq`

The `PipelineParams: PartialEq` derive depends on `EdgeMethod: PartialEq`.
`src/utils.rs` already declares `#[derive(Debug, Clone, Copy, PartialEq)]`
on `EdgeMethod`, so this works out of the box. No upstream change needed.
