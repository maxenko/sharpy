//! Example demonstrating the builder pattern for complex sharpening workflows

use sharpy::{EdgeMethod, Image};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Load an image
    let image = Image::load("tests/fixtures/lens.jpg")?;

    println!("Processing image with builder pattern...");

    // Chain multiple sharpening operations
    let result = image
        .sharpen()
        .unsharp_mask(0.8, 0.8, 2) // Subtle unsharp mask
        .edge_enhance(0.3, EdgeMethod::Sobel) // Enhance edges
        .clarity(0.4, 3.0) // Add local contrast
        .apply()?;

    // Save the result
    result.save("examples/output/lens_builder.jpg")?;

    println!("Saved enhanced image to examples/output/lens_builder.jpg");

    Ok(())
}
