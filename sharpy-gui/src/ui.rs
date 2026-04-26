use crate::state::{
    AppState, ClarityParams, EdgeParams, HighPassParams, PresetKind, StageParams, UnsharpParams,
};
use eframe::egui;
use sharpy::EdgeMethod;

/// Draws the right-side controls panel: stage groups + per-stage reset.
pub fn draw_controls(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Sharpening Pipeline");
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Stages run in fixed order: unsharp → high-pass → edges → clarity")
            .small()
            .weak(),
    );
    ui.separator();

    draw_unsharp_group(ui, &mut state.params.unsharp);
    ui.separator();
    draw_highpass_group(ui, &mut state.params.high_pass);
    ui.separator();
    draw_edges_group(ui, &mut state.params.edges);
    ui.separator();
    draw_clarity_group(ui, &mut state.params.clarity);
}

fn stage_header<T: Default>(ui: &mut egui::Ui, label: &str, stage: &mut StageParams<T>) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut stage.enabled, egui::RichText::new(label).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("↺")
                .on_hover_text("Reset this stage to defaults")
                .clicked()
            {
                stage.params = T::default();
            }
        });
    });
}

fn draw_unsharp_group(ui: &mut egui::Ui, stage: &mut StageParams<UnsharpParams>) {
    stage_header(ui, "Unsharp Mask", stage);
    ui.add_enabled_ui(stage.enabled, |ui| {
        let p = &mut stage.params;
        ui.add(
            egui::Slider::new(&mut p.radius, 0.1..=10.0)
                .logarithmic(true)
                .text("Radius"),
        );
        ui.add(egui::Slider::new(&mut p.amount, 0.0..=5.0).text("Amount"));
        ui.add(egui::Slider::new(&mut p.threshold, 0..=255).text("Threshold"));
    });
}

fn draw_highpass_group(ui: &mut egui::Ui, stage: &mut StageParams<HighPassParams>) {
    stage_header(ui, "High-Pass Sharpen", stage);
    ui.add_enabled_ui(stage.enabled, |ui| {
        ui.add(egui::Slider::new(&mut stage.params.strength, 0.05..=3.0).text("Strength"));
    });
}

fn draw_edges_group(ui: &mut egui::Ui, stage: &mut StageParams<EdgeParams>) {
    stage_header(ui, "Edge Enhance", stage);
    ui.add_enabled_ui(stage.enabled, |ui| {
        let p = &mut stage.params;
        ui.add(egui::Slider::new(&mut p.strength, 0.05..=3.0).text("Strength"));
        ui.horizontal(|ui| {
            ui.label("Method:");
            ui.radio_value(&mut p.method, EdgeMethod::Sobel, "Sobel");
            ui.radio_value(&mut p.method, EdgeMethod::Prewitt, "Prewitt");
        });
    });
}

fn draw_clarity_group(ui: &mut egui::Ui, stage: &mut StageParams<ClarityParams>) {
    stage_header(ui, "Clarity", stage);
    ui.add_enabled_ui(stage.enabled, |ui| {
        let p = &mut stage.params;
        ui.add(egui::Slider::new(&mut p.strength, 0.05..=3.0).text("Strength"));
        ui.add(egui::Slider::new(&mut p.radius, 0.5..=20.0).text("Radius"));
    });
}

pub enum TopBarAction {
    Open,
    Save,
    ResetAll,
    LoadPreset(PresetKind),
}

/// Draws the top toolbar. Returns at most one user action triggered this frame.
///
/// `selected_preset` is the preset whose values match the current params, used
/// only as the visible label of the preset combo box. Selecting any entry
/// (even the same one) returns a `LoadPreset` action; the caller decides
/// whether to apply it.
pub fn draw_top_bar(
    ui: &mut egui::Ui,
    save_in_flight: bool,
    has_source: bool,
    selected_preset: Option<PresetKind>,
) -> Option<TopBarAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button("Open…").clicked() {
            action = Some(TopBarAction::Open);
        }
        ui.add_enabled_ui(has_source && !save_in_flight, |ui| {
            if ui.button("Save As…").clicked() {
                action = Some(TopBarAction::Save);
            }
        });
        if ui.button("Reset All").clicked() {
            action = Some(TopBarAction::ResetAll);
        }
        ui.separator();
        ui.label("Preset:");
        let combo_label = selected_preset.map(|p| p.label()).unwrap_or("(custom)");
        egui::ComboBox::from_id_salt("preset_picker")
            .selected_text(combo_label)
            .show_ui(ui, |ui| {
                for preset in PresetKind::ALL {
                    let mut resp =
                        ui.selectable_label(selected_preset == Some(preset), preset.label());
                    if let Some(tip) = preset.tooltip() {
                        resp = resp.on_hover_text(tip);
                    }
                    if resp.clicked() {
                        action = Some(TopBarAction::LoadPreset(preset));
                    }
                }
            });
        if save_in_flight {
            ui.separator();
            ui.spinner();
            ui.label(egui::RichText::new("Saving…").italics());
        }
    });
    action
}

/// Drag-drop hover state for the central pane.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DropAffordance {
    /// No file is being hovered.
    Idle,
    /// A file is being hovered and a drop will be accepted.
    Active,
    /// A file is being hovered but a drop will be rejected (e.g. save in flight).
    Blocked,
}

/// Renders the central pane: preview image with drag-drop affordance.
///
/// `source_dims` is the original (non-downscaled) source size, used as the
/// upper bound for display so a small image is shown at its native size
/// rather than blown up to fill the pane. The texture itself is the
/// preview-resolution downscale.
pub fn draw_central(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    source_dims: Option<(u32, u32)>,
    drop: DropAffordance,
) {
    let available = ui.available_size();

    if let Some(tex) = texture {
        // Cap display at the source's natural size: scale up the preview
        // texture to fill the pane proportionally, but never beyond 1:1
        // with the original source pixels. Falls back to the texture's
        // own size if source dimensions aren't known yet.
        let cap = source_dims
            .map(|(w, h)| egui::vec2(w as f32, h as f32))
            .unwrap_or_else(|| tex.size_vec2());
        let scale = (available.x / cap.x).min(available.y / cap.y).min(1.0);
        let display_size = cap * scale;
        ui.centered_and_justified(|ui| {
            ui.image((tex.id(), display_size));
        });
    } else {
        ui.centered_and_justified(|ui| {
            let label = match drop {
                DropAffordance::Active => egui::RichText::new("Drop to load").size(28.0).strong(),
                DropAffordance::Blocked => egui::RichText::new("Save in progress — drop blocked")
                    .size(22.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 140, 80)),
                DropAffordance::Idle => egui::RichText::new("Drag an image here, or click Open…")
                    .size(20.0)
                    .weak(),
            };
            ui.label(label);
        });
    }

    if drop != DropAffordance::Idle {
        let stroke_color = match drop {
            DropAffordance::Active => egui::Color32::from_rgb(180, 180, 220),
            DropAffordance::Blocked => egui::Color32::from_rgb(220, 140, 80),
            DropAffordance::Idle => unreachable!(),
        };
        let rect = ui.max_rect();
        ui.painter().rect_stroke(
            rect.shrink(8.0),
            egui::CornerRadius::same(6),
            egui::Stroke::new(2.0, stroke_color),
            egui::StrokeKind::Inside,
        );
    }
}

/// Composes the status string shown at the bottom: source info + last-run + status message.
pub fn draw_status_bar(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        if let Some(source) = &state.source {
            let (sw, sh) = source.dimensions();
            ui.label(format!(
                "Source: {} ({}×{})",
                state
                    .source_path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<memory>".into()),
                sw,
                sh
            ));
        } else {
            ui.label("No image loaded.");
        }
        if let Some(preview) = &state.preview_source {
            let (pw, ph) = preview.dimensions();
            ui.label(format!("• preview {}×{}", pw, ph));
        }
        if let Some(ms) = state.last_run_ms {
            ui.label(format!("• {} ms last apply", ms));
        }

        if let Some((msg, is_error)) = state.status.current() {
            ui.separator();
            let mut text = egui::RichText::new(msg);
            if is_error {
                text = text.color(egui::Color32::from_rgb(220, 80, 80));
            } else {
                text = text.color(egui::Color32::from_rgb(120, 160, 220));
            }
            ui.label(text);
        }
    });
}
