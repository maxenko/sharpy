use crate::state::PipelineParams;
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use image::imageops::FilterType;
use image::RgbImage;
use sharpy::Image;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Hard caps copied from sharpy's lib.rs (which keeps them private). We
/// validate dimensions in the GUI *before* decoding so an oversized file
/// never blows up our memory and so the user sees a clear "too large"
/// error up front rather than a stream of "Save error" messages later.
const MAX_INPUT_PIXELS: usize = 100_000_000;
const MAX_INPUT_DIM: u32 = 65536;

/// Monotonic counter incremented every time the source image changes.
/// Worker results carry the generation they were computed against; results
/// with a stale generation are dropped by the UI so a long-running preview
/// from the previous image never lands on the new image.
pub type Generation = u64;

pub enum WorkerJob {
    Preview {
        source: Arc<RgbImage>,
        params: PipelineParams,
        generation: Generation,
    },
    Save {
        source: Arc<RgbImage>,
        params: PipelineParams,
        path: PathBuf,
    },
}

pub enum WorkerResult {
    Preview {
        image: RgbImage,
        params: PipelineParams,
        generation: Generation,
        elapsed_ms: u128,
    },
    PreviewErr {
        error: String,
        generation: Generation,
    },
    Saved {
        path: PathBuf,
        elapsed_ms: u128,
    },
    SaveErr {
        error: String,
    },
}

/// Builds a `sharpy::Image` from the Arc-shared source and runs the pipeline
/// in the GUI's fixed stage order. The Arc input is intentional: the UI
/// always retains its own clone, so `Image::from_arc_rgb` cannot avoid the
/// internal copy-on-write — the call shape stays clean either way.
pub fn build_pipeline(source: Arc<RgbImage>, params: &PipelineParams) -> sharpy::Result<Image> {
    let image = Image::from_arc_rgb(source)?;
    let mut b = image.sharpen();
    if params.unsharp.enabled {
        let u = &params.unsharp.params;
        b = b.unsharp_mask(u.radius, u.amount, u.threshold);
    }
    if params.high_pass.enabled {
        b = b.high_pass(params.high_pass.params.strength);
    }
    if params.edges.enabled {
        let e = &params.edges.params;
        b = b.edge_enhance(e.strength, e.method);
    }
    if params.clarity.enabled {
        let c = &params.clarity.params;
        b = b.clarity(c.strength, c.radius);
    }
    b.apply()
}

/// Owns the worker thread + the channels used to talk to it. The senders
/// are wrapped in `Option` so `Drop` can release them before joining the
/// handle (without that ordering, the worker's `select!` would never see
/// the receivers disconnect, and `join` would deadlock).
pub struct PreviewWorker {
    pub preview_tx: Option<Sender<WorkerJob>>,
    pub save_tx: Option<Sender<WorkerJob>>,
    pub results_rx: Receiver<WorkerResult>,
    handle: Option<thread::JoinHandle<()>>,
}

impl PreviewWorker {
    pub fn spawn(ctx: eframe::egui::Context) -> Self {
        let (preview_tx, preview_rx) = crossbeam_channel::bounded::<WorkerJob>(1);
        let (save_tx, save_rx) = unbounded::<WorkerJob>();
        let (results_tx, results_rx) = unbounded::<WorkerResult>();

        let handle = thread::Builder::new()
            .name("sharpy-gui-worker".to_string())
            .spawn(move || worker_loop(preview_rx, save_rx, results_tx, ctx))
            .expect("failed to spawn sharpy-gui worker thread");

        Self {
            preview_tx: Some(preview_tx),
            save_tx: Some(save_tx),
            results_rx,
            handle: Some(handle),
        }
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        // Drop senders first so the worker's select! returns Disconnected
        // and the loop exits. Then join.
        self.preview_tx.take();
        self.save_tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn worker_loop(
    preview_rx: Receiver<WorkerJob>,
    save_rx: Receiver<WorkerJob>,
    results_tx: Sender<WorkerResult>,
    ctx: eframe::egui::Context,
) {
    loop {
        let job = select! {
            recv(save_rx) -> msg => match msg {
                Ok(j) => j,
                Err(_) => break,
            },
            recv(preview_rx) -> msg => match msg {
                Ok(j) => j,
                Err(_) => break,
            },
        };

        // Snapshot the job's discriminator BEFORE moving the job into
        // catch_unwind, so a panic produces an error in the matching
        // category (preview panics → PreviewErr, save panics → SaveErr).
        let panic_kind = JobKind::from_job(&job);

        // catch_unwind keeps the worker alive even if the sharpy pipeline
        // panics (e.g. an internal allocation failure on a huge image).
        // Without this, a single panic would silently kill the worker, the
        // UI would never see a result, and Save would stay disabled
        // forever with no error to the user.
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| run_job(job)))
            .unwrap_or_else(|payload| panic_kind.into_error(panic_message(&*payload)));

        if results_tx.send(result).is_err() {
            // UI is gone; nothing to deliver to.
            break;
        }
        ctx.request_repaint();
    }
}

/// Kind of job in flight, captured before `catch_unwind` consumes the job.
/// Used to route a panic to the correct error variant.
enum JobKind {
    Preview(Generation),
    Save,
}

impl JobKind {
    fn from_job(job: &WorkerJob) -> Self {
        match job {
            WorkerJob::Preview { generation, .. } => JobKind::Preview(*generation),
            WorkerJob::Save { .. } => JobKind::Save,
        }
    }

    fn into_error(self, msg: String) -> WorkerResult {
        let error = format!("worker panicked: {}", msg);
        match self {
            JobKind::Preview(generation) => WorkerResult::PreviewErr { error, generation },
            JobKind::Save => WorkerResult::SaveErr { error },
        }
    }
}

/// Best-effort extraction of a panic payload's message. Panics commonly
/// carry `&'static str` or `String`; anything else falls back to a generic
/// label.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn run_job(job: WorkerJob) -> WorkerResult {
    match job {
        WorkerJob::Preview {
            source,
            params,
            generation,
        } => run_preview(source, params, generation),
        WorkerJob::Save {
            source,
            params,
            path,
        } => run_save(source, params, path),
    }
}

fn run_preview(
    source: Arc<RgbImage>,
    params: PipelineParams,
    generation: Generation,
) -> WorkerResult {
    let started = Instant::now();
    match build_pipeline(source, &params) {
        Ok(image) => WorkerResult::Preview {
            image: image.into_rgb(),
            params,
            generation,
            elapsed_ms: started.elapsed().as_millis(),
        },
        Err(e) => WorkerResult::PreviewErr {
            error: format!("preview failed: {}", e),
            generation,
        },
    }
}

fn run_save(source: Arc<RgbImage>, params: PipelineParams, path: PathBuf) -> WorkerResult {
    let started = Instant::now();
    let pipeline_result = build_pipeline(source, &params);
    let image = match pipeline_result {
        Ok(img) => img,
        Err(e) => {
            return WorkerResult::SaveErr {
                error: format!("pipeline failed: {}", e),
            };
        }
    };
    if let Err(e) = image.save(&path) {
        return WorkerResult::SaveErr {
            error: format!("save failed: {}", e),
        };
    }
    WorkerResult::Saved {
        path,
        elapsed_ms: started.elapsed().as_millis(),
    }
}

/// Reads dimensions from the file header without decoding pixels, validates
/// against sharpy's caps, then decodes. This catches oversized inputs before
/// allocating gigabytes of pixel buffer just to discover Save will reject them.
pub fn decode_and_prepare(
    path: &Path,
    preview_max_dim: u32,
) -> anyhow::Result<(RgbImage, RgbImage)> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let (w, h) = reader.into_dimensions()?;
    if w > MAX_INPUT_DIM || h > MAX_INPUT_DIM {
        anyhow::bail!(
            "image is {}x{}; max supported dimension is {}",
            w,
            h,
            MAX_INPUT_DIM
        );
    }
    let pixel_count = (w as usize).saturating_mul(h as usize);
    if pixel_count > MAX_INPUT_PIXELS {
        anyhow::bail!(
            "image has {} pixels; max supported is {} ({} MP)",
            pixel_count,
            MAX_INPUT_PIXELS,
            MAX_INPUT_PIXELS / 1_000_000
        );
    }

    let dynamic = image::open(path)?;
    let rgb = dynamic.to_rgb8();
    let preview = downscale(&rgb, preview_max_dim);
    Ok((rgb, preview))
}

fn downscale(src: &RgbImage, max_dim: u32) -> RgbImage {
    let (w, h) = src.dimensions();
    let longest = w.max(h);
    if longest <= max_dim {
        return src.clone();
    }
    let scale = max_dim as f32 / longest as f32;
    let new_w = ((w as f32) * scale).round().max(1.0) as u32;
    let new_h = ((h as f32) * scale).round().max(1.0) as u32;
    image::imageops::resize(src, new_w, new_h, FilterType::Triangle)
}
