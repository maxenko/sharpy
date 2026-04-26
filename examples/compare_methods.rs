//! Example comparing different sharpening methods on the same image

use sharpy::{EdgeMethod, Image};
use std::error::Error;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    // Load an image
    let image = Image::load("tests/fixtures/lens.jpg")?;
    let (width, height) = image.dimensions();

    println!(
        "Comparing sharpening methods on {}x{} image:\n",
        width, height
    );

    // Test unsharp mask
    let start = Instant::now();
    let unsharp = image.clone().unsharp_mask(1.0, 1.0, 0)?;
    let duration = start.elapsed();
    unsharp.save("examples/output/compare_unsharp.jpg")?;
    println!("Unsharp mask: {:?}", duration);

    // Test high-pass sharpening
    let start = Instant::now();
    let highpass = image.clone().high_pass_sharpen(0.5)?;
    let duration = start.elapsed();
    highpass.save("examples/output/compare_highpass.jpg")?;
    println!("High-pass: {:?}", duration);

    // Test edge enhancement (Sobel)
    let start = Instant::now();
    let edges_sobel = image.clone().enhance_edges(1.0, EdgeMethod::Sobel)?;
    let duration = start.elapsed();
    edges_sobel.save("examples/output/compare_edges_sobel.jpg")?;
    println!("Edge enhancement (Sobel): {:?}", duration);

    // Test edge enhancement (Prewitt)
    let start = Instant::now();
    let edges_prewitt = image.clone().enhance_edges(1.0, EdgeMethod::Prewitt)?;
    let duration = start.elapsed();
    edges_prewitt.save("examples/output/compare_edges_prewitt.jpg")?;
    println!("Edge enhancement (Prewitt): {:?}", duration);

    // Test clarity
    let start = Instant::now();
    let clarity = image.clone().clarity(1.0, 2.0)?;
    let duration = start.elapsed();
    clarity.save("examples/output/compare_clarity.jpg")?;
    println!("Clarity: {:?}", duration);

    println!("\nAll comparison images saved to examples/output/");

    Ok(())
}
