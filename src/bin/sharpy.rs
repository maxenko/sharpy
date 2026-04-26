use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use sharpy::{EdgeMethod, Image, Operation, SharpeningPresets};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "sharpy")]
#[command(author, version, about = "High-performance image sharpening tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Preview operations without processing
    #[arg(long, global = true)]
    dry_run: bool,

    /// Overwrite existing files without prompting
    #[arg(long, global = true)]
    overwrite: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply unsharp mask sharpening
    Unsharp {
        /// Input image file
        input: PathBuf,

        /// Output image file
        output: PathBuf,

        /// Blur radius (0.5-10.0)
        #[arg(short, long, default_value = "1.0")]
        radius: f32,

        /// Sharpening strength (0.0-5.0)
        #[arg(short, long, default_value = "1.0")]
        amount: f32,

        /// Minimum difference threshold (0-255)
        #[arg(short, long, default_value = "0")]
        threshold: u8,
    },

    /// Apply high-pass sharpening
    Highpass {
        /// Input image file
        input: PathBuf,

        /// Output image file
        output: PathBuf,

        /// Blend strength (0.0-3.0)
        #[arg(short, long, default_value = "0.5")]
        strength: f32,
    },

    /// Enhance edges in the image
    Edges {
        /// Input image file
        input: PathBuf,

        /// Output image file
        output: PathBuf,

        /// Enhancement strength (0.0-3.0)
        #[arg(short, long, default_value = "1.0")]
        strength: f32,

        /// Edge detection method
        #[arg(short, long, value_enum, ignore_case = true, default_value_t = EdgeMethodArg::Sobel)]
        method: EdgeMethodArg,
    },

    /// Apply clarity enhancement
    Clarity {
        /// Input image file
        input: PathBuf,

        /// Output image file
        output: PathBuf,

        /// Enhancement strength (0.0-3.0)
        #[arg(short, long, default_value = "1.0")]
        strength: f32,

        /// Local area radius (1.0-20.0)
        #[arg(short, long, default_value = "2.0")]
        radius: f32,
    },

    /// Apply a sharpening preset
    Preset {
        /// Input image file
        input: PathBuf,

        /// Output image file
        output: PathBuf,

        /// Preset name
        #[arg(short, long, value_enum, ignore_case = true)]
        preset: PresetArg,
    },

    /// Process multiple files with batch operations
    Batch {
        /// Input pattern (e.g., "*.jpg" or "images/*.png")
        pattern: String,

        /// Output directory
        #[arg(short, long)]
        output_dir: PathBuf,

        /// Output filename suffix
        #[arg(short, long, default_value = "_sharp")]
        suffix: String,

        /// Operations to apply (format: "operation:param1:param2:...")
        #[arg(short = 'p', long, value_delimiter = ',')]
        operations: Vec<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum EdgeMethodArg {
    Sobel,
    Prewitt,
}

impl From<EdgeMethodArg> for EdgeMethod {
    fn from(arg: EdgeMethodArg) -> Self {
        match arg {
            EdgeMethodArg::Sobel => EdgeMethod::Sobel,
            EdgeMethodArg::Prewitt => EdgeMethod::Prewitt,
        }
    }
}

#[derive(Clone, ValueEnum)]
enum PresetArg {
    Subtle,
    Moderate,
    Strong,
    /// Accepts either `edge-aware` (the canonical kebab-case form clap derives
    /// from `EdgeAware`) or the underscore form `edge_aware` for parity with
    /// the previous parser.
    #[value(alias = "edge_aware")]
    EdgeAware,
    Portrait,
    Landscape,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Unsharp {
            input,
            output,
            radius,
            amount,
            threshold,
        } => process_single_image(&cli, input, output, |img| {
            img.unsharp_mask(*radius, *amount, *threshold)
        }),

        Commands::Highpass {
            input,
            output,
            strength,
        } => process_single_image(&cli, input, output, |img| img.high_pass_sharpen(*strength)),

        Commands::Edges {
            input,
            output,
            strength,
            method,
        } => {
            let method = EdgeMethod::from(method.clone());
            process_single_image(&cli, input, output, |img| {
                img.enhance_edges(*strength, method)
            })
        }

        Commands::Clarity {
            input,
            output,
            strength,
            radius,
        } => process_single_image(&cli, input, output, |img| img.clarity(*strength, *radius)),

        Commands::Preset {
            input,
            output,
            preset,
        } => process_single_image(&cli, input, output, |img| apply_preset(img, preset)),

        Commands::Batch {
            pattern,
            output_dir,
            suffix,
            operations,
        } => process_batch(&cli, pattern, output_dir, suffix, operations),
    }
}

fn apply_preset(img: Image, preset: &PresetArg) -> sharpy::Result<Image> {
    let builder = match preset {
        PresetArg::Subtle => SharpeningPresets::subtle(img),
        PresetArg::Moderate => SharpeningPresets::moderate(img),
        PresetArg::Strong => SharpeningPresets::strong(img),
        PresetArg::EdgeAware => SharpeningPresets::edge_aware(img),
        PresetArg::Portrait => SharpeningPresets::portrait(img),
        PresetArg::Landscape => SharpeningPresets::landscape(img),
    };
    builder.apply()
}

fn process_single_image<F>(cli: &Cli, input: &Path, output: &Path, operation: F) -> Result<()>
where
    F: FnOnce(Image) -> sharpy::Result<Image>,
{
    if !cli.quiet {
        eprintln!("Processing: {} -> {}", input.display(), output.display());
    }

    if output.exists() && !cli.overwrite && !cli.dry_run {
        anyhow::bail!(
            "Output file already exists: {}. Use --overwrite to replace.",
            output.display()
        );
    }

    if cli.dry_run {
        if !cli.quiet {
            eprintln!(
                "Dry run: Would process {} -> {}",
                input.display(),
                output.display()
            );
        }
        return Ok(());
    }

    let image = load_image(input)?;

    if cli.verbose {
        let (width, height) = image.dimensions();
        eprintln!("Loaded image: {}x{}", width, height);
    }

    let result = operation(image).map_err(|e| anyhow::anyhow!("Processing failed: {}", e))?;

    save_image(result, output)?;

    if !cli.quiet {
        eprintln!("Successfully saved: {}", output.display());
    }

    Ok(())
}

fn process_batch(
    cli: &Cli,
    pattern: &str,
    output_dir: &Path,
    suffix: &str,
    operations: &[String],
) -> Result<()> {
    let parsed_operations = parse_operations(operations)?;

    if !cli.dry_run {
        std::fs::create_dir_all(output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                output_dir.display()
            )
        })?;
    }

    let files: Vec<_> = glob(pattern)
        .map_err(|e| anyhow::anyhow!("Invalid pattern: {}", e))?
        .filter_map(|entry| entry.ok())
        .collect();

    if files.is_empty() {
        anyhow::bail!("No files match pattern: {}", pattern);
    }

    if !cli.quiet {
        eprintln!("Found {} files to process", files.len());
    }

    let progress = (!cli.quiet)
        .then(|| -> Result<ProgressBar> {
            let pb = ProgressBar::new(files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                    )?
                    .progress_chars("#>-"),
            );
            Ok(pb)
        })
        .transpose()?;

    let mut success_count = 0;
    let mut error_count = 0;

    for path in files {
        if let Some(pb) = &progress {
            pb.set_message(format!(
                "Processing: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }

        let output_path = make_output_path(&path, output_dir, suffix)?;
        let result = process_single_with_operations(cli, &path, &output_path, &parsed_operations);

        match result {
            Ok(_) => success_count += 1,
            Err(e) => {
                error_count += 1;
                if !cli.quiet {
                    eprintln!("Error processing {}: {}", path.display(), e);
                }
            }
        }

        if let Some(pb) = &progress {
            pb.inc(1);
        }
    }

    if let Some(pb) = &progress {
        pb.finish_with_message(format!(
            "Completed: {} successful, {} errors",
            success_count, error_count
        ));
    }

    if error_count > 0 {
        anyhow::bail!("{} files failed to process", error_count);
    }

    Ok(())
}

fn make_output_path(input: &Path, output_dir: &Path, suffix: &str) -> Result<PathBuf> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid filename: {}", input.display()))?;

    let extension = input.extension().and_then(|s| s.to_str()).unwrap_or("jpg");

    Ok(output_dir.join(format!("{}{}.{}", stem, suffix, extension)))
}

fn parse_operations(operations: &[String]) -> Result<Vec<Operation>> {
    operations
        .iter()
        .map(|op| parse_single_operation(op))
        .collect()
}

fn parse_single_operation(op: &str) -> Result<Operation> {
    let parts: Vec<&str> = op.split(':').collect();

    // Slice-pattern match: each known operation gets two arms — one with the
    // exact arity that succeeds, and a fallback with a clear arity-error
    // message. Without the fallback arms, an arity-mismatched input would
    // fall through to "Unknown operation" — a regression vs. the prior
    // parser's tailored error messages.
    match parts.as_slice() {
        [name, radius, amount, threshold] if name.eq_ignore_ascii_case("unsharp") => {
            Ok(Operation::UnsharpMask {
                radius: radius.parse().context("Invalid radius")?,
                amount: amount.parse().context("Invalid amount")?,
                threshold: threshold.parse().context("Invalid threshold")?,
            })
        }
        [name, strength] if name.eq_ignore_ascii_case("highpass") => {
            Ok(Operation::HighPassSharpen {
                strength: strength.parse().context("Invalid strength")?,
            })
        }
        [name, strength, method] if name.eq_ignore_ascii_case("edges") => {
            let method = match method.to_lowercase().as_str() {
                "sobel" => EdgeMethod::Sobel,
                "prewitt" => EdgeMethod::Prewitt,
                other => anyhow::bail!("Unknown edge method: {}", other),
            };
            Ok(Operation::EnhanceEdges {
                strength: strength.parse().context("Invalid strength")?,
                method,
            })
        }
        [name, strength, radius] if name.eq_ignore_ascii_case("clarity") => {
            Ok(Operation::Clarity {
                strength: strength.parse().context("Invalid strength")?,
                radius: radius.parse().context("Invalid radius")?,
            })
        }

        [name, ..] if name.eq_ignore_ascii_case("unsharp") => {
            anyhow::bail!("Unsharp requires 3 parameters: unsharp:radius:amount:threshold")
        }
        [name, ..] if name.eq_ignore_ascii_case("highpass") => {
            anyhow::bail!("Highpass requires 1 parameter: highpass:strength")
        }
        [name, ..] if name.eq_ignore_ascii_case("edges") => {
            anyhow::bail!("Edges requires 2 parameters: edges:strength:method")
        }
        [name, ..] if name.eq_ignore_ascii_case("clarity") => {
            anyhow::bail!("Clarity requires 2 parameters: clarity:strength:radius")
        }

        [name, ..] => anyhow::bail!("Unknown operation: {}", name),
        [] => anyhow::bail!("Empty operation string"),
    }
}

fn process_single_with_operations(
    cli: &Cli,
    input: &Path,
    output: &Path,
    operations: &[Operation],
) -> Result<()> {
    if cli.dry_run {
        if cli.verbose {
            eprintln!(
                "Dry run: Would process {} -> {} with {} operations",
                input.display(),
                output.display(),
                operations.len()
            );
        }
        return Ok(());
    }

    let image = operations
        .iter()
        .try_fold(load_image(input)?, |img, op| op.apply(img))
        .map_err(|e| anyhow::anyhow!("Operation failed: {}", e))?;

    save_image(image, output)
}

fn load_image(input: &Path) -> Result<Image> {
    Image::load(input).with_context(|| format!("Failed to load image: {}", input.display()))
}

fn save_image(image: Image, output: &Path) -> Result<()> {
    image
        .save(output)
        .with_context(|| format!("Failed to save image: {}", output.display()))
}
