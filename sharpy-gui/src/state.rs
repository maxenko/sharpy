use crate::preview::Generation;
use sharpy::{EdgeMethod, Operation};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use image::RgbImage;

/// Longest side (in pixels) of the live-preview source. Full-resolution images
/// are downscaled to this once on load; all preview-time pipeline work runs
/// against the downscaled copy.
pub const PREVIEW_MAX_DIM: u32 = 768;

#[derive(Clone, PartialEq)]
pub struct UnsharpParams {
    pub radius: f32,
    pub amount: f32,
    pub threshold: u8,
}

#[derive(Clone, PartialEq)]
pub struct HighPassParams {
    pub strength: f32,
}

#[derive(Clone, PartialEq)]
pub struct EdgeParams {
    pub strength: f32,
    pub method: EdgeMethod,
}

#[derive(Clone, PartialEq)]
pub struct ClarityParams {
    pub strength: f32,
    pub radius: f32,
}

#[derive(Clone, PartialEq)]
pub struct StageParams<T> {
    pub enabled: bool,
    pub params: T,
}

#[derive(Clone, PartialEq)]
pub struct PipelineParams {
    pub unsharp: StageParams<UnsharpParams>,
    pub high_pass: StageParams<HighPassParams>,
    pub edges: StageParams<EdgeParams>,
    pub clarity: StageParams<ClarityParams>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PresetKind {
    Subtle,
    Moderate,
    Strong,
    EdgeAware,
    Portrait,
    Landscape,
}

impl PresetKind {
    pub const ALL: [PresetKind; 6] = [
        PresetKind::Subtle,
        PresetKind::Moderate,
        PresetKind::Strong,
        PresetKind::EdgeAware,
        PresetKind::Portrait,
        PresetKind::Landscape,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PresetKind::Subtle => "Subtle",
            PresetKind::Moderate => "Moderate",
            PresetKind::Strong => "Strong",
            PresetKind::EdgeAware => "Edge-Aware *",
            PresetKind::Portrait => "Portrait",
            PresetKind::Landscape => "Landscape",
        }
    }

    pub fn tooltip(self) -> Option<&'static str> {
        match self {
            PresetKind::EdgeAware => Some(
                "Parameter values match SharpeningPresets::edge_aware, but the GUI \
                 executes stages in the fixed order (unsharp → high-pass → edges → clarity), \
                 which differs from the library preset's edges → unsharp ordering. \
                 Result will not be byte-identical to CLI output.",
            ),
            _ => None,
        }
    }
}

impl Default for UnsharpParams {
    fn default() -> Self {
        Self {
            radius: 1.0,
            amount: 0.5,
            threshold: 0,
        }
    }
}

impl Default for HighPassParams {
    fn default() -> Self {
        Self { strength: 0.5 }
    }
}

impl Default for EdgeParams {
    fn default() -> Self {
        Self {
            strength: 0.5,
            method: EdgeMethod::Sobel,
        }
    }
}

impl Default for ClarityParams {
    fn default() -> Self {
        Self {
            strength: 0.5,
            radius: 3.0,
        }
    }
}

impl<T: Default> Default for StageParams<T> {
    fn default() -> Self {
        Self {
            enabled: false,
            params: T::default(),
        }
    }
}

impl<T> StageParams<T> {
    /// Constructs a stage that is enabled with the given parameters.
    pub fn on(params: T) -> Self {
        Self {
            enabled: true,
            params,
        }
    }
}

impl PipelineParams {
    /// All-stages-disabled defaults. Useful as a base that presets layer onto.
    pub fn empty() -> Self {
        Self {
            unsharp: StageParams::default(),
            high_pass: StageParams::default(),
            edges: StageParams::default(),
            clarity: StageParams::default(),
        }
    }

    pub fn from_preset(preset: PresetKind) -> Self {
        let mut p = Self::empty();
        match preset {
            PresetKind::Subtle => {
                p.unsharp = StageParams::on(UnsharpParams {
                    radius: 0.8,
                    amount: 0.6,
                    threshold: 2,
                });
            }
            PresetKind::Moderate => {
                p.unsharp = StageParams::on(UnsharpParams {
                    radius: 1.0,
                    amount: 1.0,
                    threshold: 3,
                });
                p.clarity = StageParams::on(ClarityParams {
                    strength: 0.3,
                    radius: 2.0,
                });
            }
            PresetKind::Strong => {
                p.unsharp = StageParams::on(UnsharpParams {
                    radius: 1.5,
                    amount: 1.5,
                    threshold: 2,
                });
                p.high_pass = StageParams::on(HighPassParams { strength: 0.3 });
                p.clarity = StageParams::on(ClarityParams {
                    strength: 0.5,
                    radius: 3.0,
                });
            }
            PresetKind::EdgeAware => {
                p.edges = StageParams::on(EdgeParams {
                    strength: 0.8,
                    method: EdgeMethod::Sobel,
                });
                p.unsharp = StageParams::on(UnsharpParams {
                    radius: 0.5,
                    amount: 0.8,
                    threshold: 5,
                });
            }
            PresetKind::Portrait => {
                p.unsharp = StageParams::on(UnsharpParams {
                    radius: 1.2,
                    amount: 0.7,
                    threshold: 10,
                });
                p.clarity = StageParams::on(ClarityParams {
                    strength: 0.2,
                    radius: 5.0,
                });
            }
            PresetKind::Landscape => {
                p.unsharp = StageParams::on(UnsharpParams {
                    radius: 1.0,
                    amount: 1.2,
                    threshold: 1,
                });
                p.edges = StageParams::on(EdgeParams {
                    strength: 0.5,
                    method: EdgeMethod::Sobel,
                });
                p.clarity = StageParams::on(ClarityParams {
                    strength: 0.4,
                    radius: 3.0,
                });
            }
        }
        p
    }

    /// Returns the preset whose factory values match the current params,
    /// or `None` if the user has hand-tuned the sliders. Used to label the
    /// preset combo box.
    pub fn matching_preset(&self) -> Option<PresetKind> {
        PresetKind::ALL
            .iter()
            .copied()
            .find(|p| Self::from_preset(*p) == *self)
    }

    /// Operations the GUI would emit, in the GUI's fixed pipeline order.
    /// Used for the preset-drift smoke test.
    #[allow(dead_code)]
    pub fn expected_operations(&self) -> Vec<Operation> {
        let mut ops = Vec::new();
        if self.unsharp.enabled {
            let u = &self.unsharp.params;
            ops.push(Operation::UnsharpMask {
                radius: u.radius,
                amount: u.amount,
                threshold: u.threshold,
            });
        }
        if self.high_pass.enabled {
            ops.push(Operation::HighPassSharpen {
                strength: self.high_pass.params.strength,
            });
        }
        if self.edges.enabled {
            let e = &self.edges.params;
            ops.push(Operation::EnhanceEdges {
                strength: e.strength,
                method: e.method,
            });
        }
        if self.clarity.enabled {
            let c = &self.clarity.params;
            ops.push(Operation::Clarity {
                strength: c.strength,
                radius: c.radius,
            });
        }
        ops
    }
}

impl Default for PipelineParams {
    fn default() -> Self {
        Self::from_preset(PresetKind::Moderate)
    }
}

/// Status bar state. Two slots: `info` (transient, auto-expires) and
/// `error` (sticky, cleared only by an in-category success or an explicit
/// dismiss).
///
/// Internally `info` stores the **expiry** instant (not a "set at" instant)
/// so both query paths (`current`, `next_expiry`) compute time-remaining
/// against the same source of truth — no off-by-one truncation.
#[derive(Default)]
pub struct StatusBar {
    info: Option<(String, Instant)>,
    error: Option<(StatusErrorCategory, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusErrorCategory {
    Load,
    Save,
    /// App-fatal errors (worker thread gone, channel disconnected). These
    /// are not cleared by a successful Load or Save; the user typically has
    /// to restart.
    Worker,
}

impl StatusBar {
    pub const INFO_TTL: std::time::Duration = std::time::Duration::from_secs(5);

    pub fn info(&mut self, msg: impl Into<String>) {
        let expires_at = Instant::now() + Self::INFO_TTL;
        self.info = Some((msg.into(), expires_at));
    }

    /// Returns how long until the current info message expires, if any.
    /// Used to schedule a repaint that visibly clears the bar.
    pub fn next_expiry(&self) -> Option<std::time::Duration> {
        let (_, expires_at) = self.info.as_ref()?;
        expires_at.checked_duration_since(Instant::now())
    }

    pub fn error(&mut self, category: StatusErrorCategory, msg: impl Into<String>) {
        self.error = Some((category, msg.into()));
    }

    pub fn clear_error(&mut self, category: StatusErrorCategory) {
        if matches!(&self.error, Some((c, _)) if *c == category) {
            self.error = None;
        }
    }

    /// Returns the current message to render, plus whether it's an error.
    /// Drops the info entry if it has expired; errors are sticky.
    pub fn current(&mut self) -> Option<(String, bool)> {
        if let Some((cat, msg)) = &self.error {
            return Some((format!("[{}] {}", cat.label(), msg), true));
        }
        let now = Instant::now();
        match self.info.as_ref() {
            Some((msg, expires_at)) if *expires_at > now => Some((msg.clone(), false)),
            Some(_) => {
                self.info = None;
                None
            }
            None => None,
        }
    }
}

impl StatusErrorCategory {
    fn label(self) -> &'static str {
        match self {
            StatusErrorCategory::Load => "Load error",
            StatusErrorCategory::Save => "Save error",
            StatusErrorCategory::Worker => "Worker error",
        }
    }
}

/// Top-level mutable state owned by the App. The `Default` impl loads the
/// Moderate preset (via `PipelineParams::default`).
#[derive(Default)]
pub struct AppState {
    pub source: Option<Arc<RgbImage>>,
    pub source_path: Option<PathBuf>,
    pub preview_source: Option<Arc<RgbImage>>,
    pub last_run_ms: Option<u128>,
    pub params: PipelineParams,
    pub last_sent_params: Option<PipelineParams>,
    pub pending: Option<PipelineParams>,
    pub save_in_flight: bool,
    pub status: StatusBar,
    /// Incremented every time the source image changes. Worker preview
    /// results that don't match the current generation are dropped so a
    /// long-running preview from the previous image never lands on the
    /// new image.
    pub source_generation: Generation,
}

impl AppState {
    /// True if the params have changed since the last job we sent the worker.
    pub fn params_dirty(&self) -> bool {
        self.last_sent_params.as_ref() != Some(&self.params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::build_pipeline;
    use sharpy::SharpeningPresets;

    fn dummy_image() -> sharpy::Image {
        sharpy::Image::from_rgb(image::RgbImage::new(1, 1)).unwrap()
    }

    #[test]
    fn each_preset_produces_valid_output() {
        let source = Arc::new(image::RgbImage::new(50, 50));
        for preset in PresetKind::ALL {
            let params = PipelineParams::from_preset(preset);
            assert!(
                build_pipeline(source.clone(), &params).is_ok(),
                "preset {:?} failed",
                preset
            );
        }
    }

    #[test]
    fn gui_presets_match_library_presets() {
        for preset in PresetKind::ALL {
            let lib_ops = match preset {
                PresetKind::Subtle => SharpeningPresets::subtle(dummy_image()),
                PresetKind::Moderate => SharpeningPresets::moderate(dummy_image()),
                PresetKind::Strong => SharpeningPresets::strong(dummy_image()),
                PresetKind::EdgeAware => SharpeningPresets::edge_aware(dummy_image()),
                PresetKind::Portrait => SharpeningPresets::portrait(dummy_image()),
                PresetKind::Landscape => SharpeningPresets::landscape(dummy_image()),
            }
            .operations()
            .to_vec();

            let gui_ops = PipelineParams::from_preset(preset).expected_operations();

            // Order-independent comparison: edge_aware diverges in execution
            // order between GUI (fixed) and library (preset-defined). The
            // multiset of operations must still match.
            let mut a = lib_ops.clone();
            let mut b = gui_ops.clone();
            a.sort_by_key(|op| op.name());
            b.sort_by_key(|op| op.name());
            assert_eq!(a, b, "preset {:?} drift between GUI and library", preset);
        }
    }
}
