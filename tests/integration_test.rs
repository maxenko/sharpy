use image::{Rgb, RgbImage};
use sharpy::{EdgeMethod, Image};

/// Create a test image with patterns to test sharpening
fn create_test_image() -> RgbImage {
    let mut img = RgbImage::new(256, 256);

    // Create a pattern with edges and gradients
    for y in 0..256 {
        for x in 0..256 {
            let value = if x < 128 {
                if y < 128 {
                    // Checkerboard pattern in top-left
                    if (x / 16 + y / 16) % 2 == 0 {
                        64
                    } else {
                        192
                    }
                } else {
                    // Vertical gradient in bottom-left
                    (x * 2) as u8
                }
            } else {
                if y < 128 {
                    // Horizontal gradient in top-right
                    y as u8
                } else {
                    // Solid color in bottom-right
                    128
                }
            };

            img.put_pixel(x as u32, y as u32, Rgb([value, value, value]));
        }
    }

    img
}

/// Calculate mean squared error between two images
fn calculate_mse(img1: &RgbImage, img2: &RgbImage) -> f64 {
    assert_eq!(img1.dimensions(), img2.dimensions());

    let mut sum = 0.0;
    let pixels = (img1.width() * img1.height()) as f64;

    for (p1, p2) in img1.pixels().zip(img2.pixels()) {
        for i in 0..3 {
            let diff = p1[i] as f64 - p2[i] as f64;
            sum += diff * diff;
        }
    }

    sum / (pixels * 3.0)
}

/// Check if sharpening actually increases edge contrast
fn measure_edge_strength(img: &RgbImage) -> f64 {
    let (width, height) = img.dimensions();
    let mut edge_sum = 0.0;
    let mut count = 0;

    // Simple edge detection using pixel differences
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let center = img.get_pixel(x, y)[0] as f64;
            let right = img.get_pixel(x + 1, y)[0] as f64;
            let bottom = img.get_pixel(x, y + 1)[0] as f64;

            let edge_strength = ((center - right).abs() + (center - bottom).abs()) / 2.0;
            edge_sum += edge_strength;
            count += 1;
        }
    }

    edge_sum / count as f64
}

#[test]
fn test_unsharp_mask_increases_sharpness() {
    let test_img = create_test_image();
    let image = Image::from_rgb(test_img.clone()).unwrap();

    let sharpened = image.unsharp_mask(1.0, 1.0, 0).unwrap();
    let sharpened_rgb = sharpened.into_rgb();

    let original_edges = measure_edge_strength(&test_img);
    let sharpened_edges = measure_edge_strength(&sharpened_rgb);

    // Sharpening should increase edge strength
    assert!(
        sharpened_edges > original_edges * 1.1,
        "Edge strength should increase by at least 10% (original: {}, sharpened: {})",
        original_edges,
        sharpened_edges
    );
}

#[test]
fn test_high_pass_sharpen_effect() {
    let test_img = create_test_image();
    let image = Image::from_rgb(test_img.clone()).unwrap();

    let sharpened = image.high_pass_sharpen(0.7).unwrap();
    let sharpened_rgb = sharpened.into_rgb();

    // High-pass should modify the image
    let mse = calculate_mse(&test_img, &sharpened_rgb);
    assert!(
        mse > 10.0,
        "High-pass sharpening should significantly modify the image (MSE: {})",
        mse
    );

    // But not too much
    assert!(
        mse < 5000.0,
        "High-pass sharpening should not destroy the image (MSE: {})",
        mse
    );
}

#[test]
fn test_edge_enhancement_methods() {
    let test_img = create_test_image();

    // Test both Sobel and Prewitt methods
    for method in [EdgeMethod::Sobel, EdgeMethod::Prewitt] {
        let image = Image::from_rgb(test_img.clone()).unwrap();
        let enhanced = image.enhance_edges(1.0, method).unwrap();
        let enhanced_rgb = enhanced.into_rgb();

        let original_edges = measure_edge_strength(&test_img);
        let enhanced_edges = measure_edge_strength(&enhanced_rgb);

        assert!(
            enhanced_edges > original_edges,
            "Edge enhancement with {:?} should increase edge strength",
            method
        );
    }
}

#[test]
fn test_clarity_enhancement() {
    let test_img = create_test_image();
    let image = Image::from_rgb(test_img.clone()).unwrap();

    let enhanced = image.clarity(1.0, 3.0).unwrap();
    let enhanced_rgb = enhanced.into_rgb();

    // Clarity should modify the image
    let mse = calculate_mse(&test_img, &enhanced_rgb);
    assert!(mse > 5.0, "Clarity should modify the image (MSE: {})", mse);
    assert!(
        mse < 1000.0,
        "Clarity should not destroy the image (MSE: {})",
        mse
    );
}

#[test]
fn test_chained_operations() {
    let test_img = create_test_image();
    let image = Image::from_rgb(test_img.clone()).unwrap();

    // Apply multiple operations
    let result = image
        .unsharp_mask(0.5, 0.5, 5)
        .unwrap()
        .high_pass_sharpen(0.3)
        .unwrap()
        .clarity(0.5, 2.0)
        .unwrap();

    let result_rgb = result.into_rgb();

    // Check that the image is still valid
    assert_eq!(result_rgb.dimensions(), test_img.dimensions());

    // Check that operations had cumulative effect
    let edge_strength = measure_edge_strength(&result_rgb);
    let original_strength = measure_edge_strength(&test_img);
    assert!(
        edge_strength > original_strength,
        "Chained operations should enhance edges"
    );
}

#[test]
fn test_parameter_bounds() {
    let test_img = create_test_image();

    // Test with extreme but valid parameters
    let test_cases = vec![
        ("min radius", 0.5, 1.0, 0),
        ("max radius", 10.0, 1.0, 0),
        ("min amount", 1.0, 0.0, 0),
        ("max amount", 1.0, 5.0, 0),
        ("max threshold", 1.0, 1.0, 255),
    ];

    for (name, radius, amount, threshold) in test_cases {
        let image = Image::from_rgb(test_img.clone()).unwrap();
        let result = image.unsharp_mask(radius, amount, threshold);
        assert!(
            result.is_ok(),
            "Operation '{}' should succeed with valid parameters",
            name
        );
    }
}

#[test]
fn test_image_dimensions_preserved() {
    let sizes = vec![(100, 100), (256, 128), (333, 444)];

    for (width, height) in sizes {
        let img = RgbImage::new(width, height);
        let image = Image::from_rgb(img).unwrap();

        // Test all operations preserve dimensions
        #[allow(clippy::type_complexity)]
        let operations: Vec<(&str, Box<dyn Fn(Image) -> sharpy::Result<Image>>)> = vec![
            (
                "unsharp_mask",
                Box::new(|img| img.unsharp_mask(1.0, 1.0, 0)),
            ),
            ("high_pass", Box::new(|img| img.high_pass_sharpen(0.5))),
            (
                "edge_enhance",
                Box::new(|img| img.enhance_edges(1.0, EdgeMethod::Sobel)),
            ),
            ("clarity", Box::new(|img| img.clarity(1.0, 2.0))),
        ];

        for (name, op) in operations {
            let result = op(image.clone()).unwrap();
            let result_rgb = result.into_rgb();
            assert_eq!(
                result_rgb.dimensions(),
                (width, height),
                "{} should preserve dimensions",
                name
            );
        }
    }
}

#[test]
fn test_builder_pattern_integration() {
    let test_img = create_test_image();
    let image = Image::from_rgb(test_img.clone()).unwrap();

    let result = image
        .sharpen()
        .unsharp_mask(0.8, 0.8, 2)
        .edge_enhance(0.3, EdgeMethod::Sobel)
        .clarity(0.4, 2.5)
        .apply()
        .unwrap();

    let result_rgb = result.into_rgb();

    // Verify the builder applied all operations
    let edge_strength = measure_edge_strength(&result_rgb);
    let original_strength = measure_edge_strength(&test_img);
    assert!(
        edge_strength > original_strength * 1.2,
        "Builder pattern should apply all sharpening operations"
    );
}

#[test]
fn test_preset_integration() {
    use sharpy::SharpeningPresets;

    let test_img = create_test_image();
    let image = Image::from_rgb(test_img).unwrap();

    // Test all presets
    let presets = vec![
        ("subtle", SharpeningPresets::subtle(image.clone())),
        ("moderate", SharpeningPresets::moderate(image.clone())),
        ("strong", SharpeningPresets::strong(image.clone())),
        ("edge_aware", SharpeningPresets::edge_aware(image.clone())),
        ("portrait", SharpeningPresets::portrait(image.clone())),
        ("landscape", SharpeningPresets::landscape(image.clone())),
    ];

    for (name, builder) in presets {
        let result = builder.apply();
        assert!(
            result.is_ok(),
            "Preset '{}' should apply successfully",
            name
        );
    }
}

#[test]
fn test_memory_bounds_checking() {
    // Test that extremely large images are rejected
    let huge_image = RgbImage::new(100000, 100000); // 10 billion pixels
    let result = Image::from_rgb(huge_image);
    assert!(
        result.is_err(),
        "Should reject images exceeding memory limits"
    );

    // Test dimension limits
    let tall_image = RgbImage::new(100, 70000); // Exceeds max dimension
    let result = Image::from_rgb(tall_image);
    assert!(
        result.is_err(),
        "Should reject images exceeding dimension limits"
    );

    // Test that reasonable images are accepted
    let normal_image = RgbImage::new(4096, 4096); // 16 megapixels
    let result = Image::from_rgb(normal_image);
    assert!(result.is_ok(), "Should accept reasonably sized images");
}
