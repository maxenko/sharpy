//! Basic example showing simple unsharp mask sharpening

use sharpy::Image;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Load an image
    let image = Image::load("tests/fixtures/lens.jpg")?;

    println!("Loaded image with dimensions: {:?}", image.dimensions());

    // Apply unsharp mask with default settings
    let sharpened = image.unsharp_mask(
        1.0, // radius
        1.0, // amount
        0,   // threshold
    )?;

    // Save the result
    sharpened.save("examples/output/lens_sharpened.jpg")?;

    println!("Saved sharpened image to examples/output/lens_sharpened.jpg");

    Ok(())
}
