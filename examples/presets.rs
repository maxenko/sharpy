//! Example showing how to use built-in sharpening presets

use sharpy::{Image, SharpeningPresets};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Load an image
    let image = Image::load("tests/fixtures/lens.jpg")?;

    // Try different presets
    #[allow(clippy::type_complexity)]
    let presets: &[(&str, fn(Image) -> sharpy::Result<Image>)] = &[
        ("subtle", |img| SharpeningPresets::subtle(img).apply()),
        ("moderate", |img| SharpeningPresets::moderate(img).apply()),
        ("strong", |img| SharpeningPresets::strong(img).apply()),
        ("portrait", |img| SharpeningPresets::portrait(img).apply()),
        ("landscape", |img| SharpeningPresets::landscape(img).apply()),
        ("edge_aware", |img| {
            SharpeningPresets::edge_aware(img).apply()
        }),
    ];

    for (name, preset_fn) in presets {
        println!("Applying {} preset...", name);

        let result = preset_fn(image.clone())?;
        let output_path = format!("examples/output/lens_{}.jpg", name);

        result.save(&output_path)?;
        println!("  Saved to {}", output_path);
    }

    Ok(())
}
