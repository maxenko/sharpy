use crate::preview::{decode_and_prepare, PreviewWorker, WorkerJob, WorkerResult};
use crate::state::{AppState, PipelineParams, StatusErrorCategory, PREVIEW_MAX_DIM};
use crate::ui;
use crossbeam_channel::TrySendError;
use eframe::egui;
use image::RgbImage;
use std::path::Path;
use std::sync::Arc;

pub struct App {
    state: AppState,
    worker: PreviewWorker,
    preview_texture: Option<egui::TextureHandle>,
    current_texture_dims: Option<(u32, u32)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let worker = PreviewWorker::spawn(cc.egui_ctx.clone());
        Self {
            state: AppState::default(),
            worker,
            preview_texture: None,
            current_texture_dims: None,
        }
    }

    fn try_load_image(&mut self, path: &Path, extras_dropped: usize) {
        if self.state.save_in_flight {
            self.state
                .status
                .info("Save in progress; please wait before loading.");
            return;
        }
        match decode_and_prepare(path, PREVIEW_MAX_DIM) {
            Ok((source, preview)) => {
                let (sw, sh) = source.dimensions();
                let (pw, ph) = preview.dimensions();
                self.state.source = Some(Arc::new(source));
                self.state.preview_source = Some(Arc::new(preview));
                self.state.source_path = Some(path.to_path_buf());
                self.state.last_sent_params = None;
                self.state.pending = None;
                self.state.last_run_ms = None;
                self.state.source_generation = self.state.source_generation.wrapping_add(1);
                self.state.status.clear_error(StatusErrorCategory::Load);

                let extras_suffix = if extras_dropped > 0 {
                    format!(" — {} other file(s) ignored", extras_dropped)
                } else {
                    String::new()
                };
                self.state.status.info(format!(
                    "Loaded {} ({}×{}) — preview {}×{}{}",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "<file>".into()),
                    sw,
                    sh,
                    pw,
                    ph,
                    extras_suffix
                ));

                // Force the texture to be re-uploaded with new dimensions.
                self.preview_texture = None;
                self.current_texture_dims = None;
            }
            Err(e) => {
                self.state.status.error(
                    StatusErrorCategory::Load,
                    format!("Could not load {}: {}", path.display(), e),
                );
            }
        }
    }

    fn try_save_to(&mut self, path: std::path::PathBuf) {
        if self.state.save_in_flight {
            return;
        }
        let Some(source) = self.state.source.clone() else {
            return;
        };
        let job = WorkerJob::Save {
            source,
            params: self.state.params.clone(),
            path,
        };
        if let Some(sender) = &self.worker.save_tx {
            match sender.send(job) {
                Ok(()) => {
                    self.state.save_in_flight = true;
                    self.state.status.clear_error(StatusErrorCategory::Save);
                }
                Err(_) => {
                    self.state.status.error(
                        StatusErrorCategory::Worker,
                        "Worker thread is gone; restart the app.",
                    );
                }
            }
        }
    }

    fn pump_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.worker.results_rx.try_recv() {
            match result {
                WorkerResult::Preview {
                    image,
                    params,
                    generation,
                    elapsed_ms,
                } => {
                    // Drop results from a previous source — the user has
                    // since loaded a new image and the worker's pixels no
                    // longer correspond to what's on screen.
                    if generation != self.state.source_generation {
                        continue;
                    }
                    self.upload_preview(ctx, &image);
                    self.state.last_run_ms = Some(elapsed_ms);
                    self.state.last_sent_params = Some(params);
                }
                WorkerResult::PreviewErr { error, generation } => {
                    if generation != self.state.source_generation {
                        continue;
                    }
                    // Preview pipeline failures should be hard to miss —
                    // they signal a programmer bug or library regression
                    // (slider ranges already clamp valid input). Sticky
                    // error in the Worker category, not auto-expiring info.
                    self.state.status.error(
                        StatusErrorCategory::Worker,
                        format!("Preview error: {}", error),
                    );
                }
                WorkerResult::Saved { path, elapsed_ms } => {
                    self.state.save_in_flight = false;
                    self.state.status.info(format!(
                        "Saved {} ({:.1} s)",
                        path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "<file>".into()),
                        elapsed_ms as f64 / 1000.0
                    ));
                }
                WorkerResult::SaveErr { error } => {
                    self.state.save_in_flight = false;
                    self.state.status.error(StatusErrorCategory::Save, error);
                }
            }
        }
    }

    fn upload_preview(&mut self, ctx: &egui::Context, image: &RgbImage) {
        let (w, h) = image.dimensions();
        let dims = (w, h);
        let color = egui::ColorImage::from_rgb([w as usize, h as usize], image.as_raw());
        let opts = egui::TextureOptions::LINEAR;

        let needs_new_handle = self.current_texture_dims != Some(dims);
        if needs_new_handle || self.preview_texture.is_none() {
            self.preview_texture = Some(ctx.load_texture("sharpy-preview", color, opts));
            self.current_texture_dims = Some(dims);
        } else if let Some(handle) = &mut self.preview_texture {
            handle.set(color, opts);
        }
    }

    fn dispatch_pending_preview(&mut self) {
        let Some(source) = &self.state.preview_source else {
            return;
        };
        if !self.state.params_dirty() && self.state.pending.is_none() {
            return;
        }

        let params = self
            .state
            .pending
            .take()
            .unwrap_or_else(|| self.state.params.clone());

        let job = WorkerJob::Preview {
            source: source.clone(),
            params: params.clone(),
            generation: self.state.source_generation,
        };

        let Some(sender) = &self.worker.preview_tx else {
            return;
        };
        match sender.try_send(job) {
            Ok(()) => {
                self.state.last_sent_params = Some(params);
            }
            Err(TrySendError::Full(_)) => {
                // Worker is busy; stash the latest params. The worker calls
                // request_repaint() when it finishes, so the next frame
                // will retry — no manual repaint scheduling needed.
                self.state.pending = Some(params);
            }
            Err(TrySendError::Disconnected(_)) => {
                self.state.status.error(
                    StatusErrorCategory::Worker,
                    "Preview worker disconnected. Restart the app.",
                );
            }
        }
    }

    /// Reads drag-drop state from egui. Returns whether a file is currently
    /// being hovered over the window so the central pane can render a
    /// "Drop to load" affordance.
    fn handle_drag_drop(&mut self, ctx: &egui::Context) -> bool {
        let (hovering, first_dropped) = ctx.input(|i| {
            let hovering = !i.raw.hovered_files.is_empty();
            let first = i.raw.dropped_files.first().cloned();
            let extras = i.raw.dropped_files.len().saturating_sub(1);
            (hovering, first.map(|f| (f, extras)))
        });
        if let Some((file, extras)) = first_dropped {
            if let Some(path) = &file.path {
                let path = path.clone();
                self.try_load_image(&path, extras);
            }
        }
        hovering
    }

    fn handle_top_bar_action(&mut self, action: ui::TopBarAction) {
        match action {
            ui::TopBarAction::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "Image",
                        &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"],
                    )
                    .set_title("Open image")
                    .pick_file()
                {
                    self.try_load_image(&path, 0);
                }
            }
            ui::TopBarAction::Save => {
                let default_name = self
                    .state
                    .source_path
                    .as_ref()
                    .and_then(|p| p.file_stem())
                    .map(|s| format!("{}_sharp.jpg", s.to_string_lossy()))
                    .unwrap_or_else(|| "sharpened.jpg".into());

                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("JPEG", &["jpg", "jpeg"])
                    .add_filter("PNG", &["png"])
                    .add_filter("TIFF", &["tif", "tiff"])
                    .set_file_name(&default_name)
                    .set_title("Save sharpened image")
                    .save_file()
                {
                    self.try_save_to(path);
                }
            }
            ui::TopBarAction::ResetAll => {
                self.state.params = PipelineParams::default();
            }
            ui::TopBarAction::LoadPreset(preset) => {
                self.state.params = PipelineParams::from_preset(preset);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Drain worker results first so subsequent UI reflects the latest
        // preview.
        self.pump_results(&ctx);

        let selected_preset = self.state.params.matching_preset();
        let action = egui::Panel::top("top_bar")
            .show_inside(ui, |ui| {
                ui.add_space(4.0);
                let action = ui::draw_top_bar(
                    ui,
                    self.state.save_in_flight,
                    self.state.source.is_some(),
                    selected_preset,
                );
                ui.add_space(4.0);
                action
            })
            .inner;
        if let Some(a) = action {
            self.handle_top_bar_action(a);
        }

        egui::Panel::bottom("status_bar").show_inside(ui, |ui| {
            ui::draw_status_bar(ui, &mut self.state);
        });

        egui::Panel::right("controls")
            .resizable(true)
            .default_size(340.0)
            .min_size(280.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui::draw_controls(ui, &mut self.state);
                });
            });

        let hovering = self.handle_drag_drop(&ctx);
        // While save is in flight we ignore drops anyway (try_load_image
        // gates on save_in_flight) — show that to the user via the hover
        // overlay.
        let drop_state = if self.state.save_in_flight && hovering {
            ui::DropAffordance::Blocked
        } else if hovering {
            ui::DropAffordance::Active
        } else {
            ui::DropAffordance::Idle
        };

        // The remaining space inside `ui` IS the central pane.
        let source_dims = self.state.source.as_ref().map(|s| s.dimensions());
        ui::draw_central(ui, self.preview_texture.as_ref(), source_dims, drop_state);

        // Run dispatch after UI so any param changes from this frame are
        // reflected immediately.
        self.dispatch_pending_preview();

        // Schedule a repaint exactly when the status info expires, so the
        // bar visibly clears itself rather than waiting for the next user
        // input.
        if let Some(remaining) = self.state.status.next_expiry() {
            ctx.request_repaint_after(remaining);
        }
    }
}
