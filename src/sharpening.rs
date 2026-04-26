use crate::utils::{
    apply_convolution, apply_edge_detection, blend_images, calculate_luminance, clamp_u8,
    gaussian_blur, EdgeMethod, HIGH_PASS_KERNEL,
};
use crate::{Image, Result};
use rayon::prelude::*;
use std::sync::Arc;

/// Applies unsharp masking to sharpen an image.
///
/// # Parameters
/// - `radius`: Blur radius for the mask (0.5-10.0)
/// - `amount`: Strength of sharpening (0.0-5.0)
/// - `threshold`: Minimum difference to apply sharpening (0-255)
pub fn unsharp_mask(mut image: Image, radius: f32, amount: f32, threshold: u8) -> Result<Image> {
    // Snapshot the original before mutating, so the parallel pass below reads
    // a stable view independent of the in-place writes into `buffer`.
    let original = Arc::new(image.data.get_ref().clone());
    let blurred = Arc::new(gaussian_blur(&original, radius));
    let threshold_f = threshold as f32;

    let buffer = image.data.get_mut();

    buffer
        .enumerate_rows_mut()
        .par_bridge()
        .for_each(|(y, row)| {
            for (x, _, pixel) in row {
                let orig_pixel = original.get_pixel(x, y);
                let blur_pixel = blurred.get_pixel(x, y);

                for i in 0..3 {
                    let orig_val = orig_pixel[i] as f32;
                    let diff = orig_val - blur_pixel[i] as f32;

                    pixel[i] = if diff.abs() > threshold_f {
                        clamp_u8(orig_val + diff * amount)
                    } else {
                        orig_pixel[i]
                    };
                }
            }
        });

    Ok(image)
}

/// Applies high-pass sharpening using a fixed 3x3 convolution kernel.
///
/// # Parameters
/// - `strength`: Blend strength with original image (0.0-3.0)
pub fn high_pass_sharpen(mut image: Image, strength: f32) -> Result<Image> {
    let original = image.data.get_ref().clone();
    let sharpened = apply_convolution(&original, &HIGH_PASS_KERNEL, 3);

    let buffer = image.data.get_mut();
    *buffer = blend_images(&original, &sharpened, strength);

    Ok(image)
}

/// Enhances edges in an image using edge detection.
///
/// # Parameters
/// - `strength`: Edge enhancement strength (0.0-3.0)
/// - `method`: Edge detection method (Sobel or Prewitt)
pub fn enhance_edges(mut image: Image, strength: f32, method: EdgeMethod) -> Result<Image> {
    let original = Arc::new(image.data.get_ref().clone());
    let edges = Arc::new(apply_edge_detection(&original, method));

    let buffer = image.data.get_mut();

    buffer
        .enumerate_rows_mut()
        .par_bridge()
        .for_each(|(y, row)| {
            for (x, _, pixel) in row {
                let orig_pixel = original.get_pixel(x, y);
                let edge_strength = calculate_luminance(edges.get_pixel(x, y)) / 255.0;
                let enhancement = edge_strength * strength;
                let edge_boost = edge_strength * 255.0 * enhancement;

                for i in 0..3 {
                    pixel[i] = clamp_u8(orig_pixel[i] as f32 + edge_boost);
                }
            }
        });

    Ok(image)
}

/// Applies clarity enhancement to improve local contrast.
///
/// # Parameters
/// - `strength`: Enhancement strength (0.0-3.0)
/// - `radius`: Local area radius (1.0-20.0)
pub fn clarity(mut image: Image, strength: f32, radius: f32) -> Result<Image> {
    let original = Arc::new(image.data.get_ref().clone());
    let (width, height) = original.dimensions();

    let buffer = image.data.get_mut();

    let window_size = (radius * 2.0).round() as usize;
    let half_window = window_size / 2;

    buffer
        .enumerate_rows_mut()
        .par_bridge()
        .for_each(|(y, row)| {
            for (x, _, pixel) in row {
                let orig_pixel = original.get_pixel(x, y);
                let orig_luminance = calculate_luminance(orig_pixel);

                let mut local_sum = 0.0;
                let mut count = 0;

                for dy in -(half_window as i32)..=(half_window as i32) {
                    for dx in -(half_window as i32)..=(half_window as i32) {
                        let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;
                        let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;
                        local_sum += calculate_luminance(original.get_pixel(nx, ny));
                        count += 1;
                    }
                }

                let local_avg = local_sum / count as f32;
                let contrast_diff = orig_luminance - local_avg;

                // Boost midtones harder than shadows/highlights to avoid crushing extremes.
                let midtone_factor = if orig_luminance > 64.0 && orig_luminance < 192.0 {
                    1.0
                } else {
                    0.5
                };

                let enhancement = contrast_diff * strength * midtone_factor * 0.5;

                for i in 0..3 {
                    pixel[i] = clamp_u8(orig_pixel[i] as f32 + enhancement);
                }
            }
        });

    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;
    use image::{Rgb, RgbImage};

    fn create_test_image() -> Image {
        let mut img = RgbImage::new(100, 100);

        for y in 0..100 {
            for x in 0..100 {
                let value = if (x / 10 + y / 10) % 2 == 0 { 100 } else { 200 };
                img.put_pixel(x, y, Rgb([value, value, value]));
            }
        }

        Image::from_rgb(img).unwrap()
    }

    #[test]
    fn test_unsharp_mask() {
        let img = create_test_image();
        let result = unsharp_mask(img, 1.0, 1.0, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_high_pass_sharpen() {
        let img = create_test_image();
        let result = high_pass_sharpen(img, 0.5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_enhance_edges() {
        let img = create_test_image();
        let result = enhance_edges(img, 1.0, EdgeMethod::Sobel);
        assert!(result.is_ok());
    }

    #[test]
    fn test_clarity() {
        let img = create_test_image();
        let result = clarity(img, 1.0, 2.0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_chain_operations() {
        let img = create_test_image();
        let result = unsharp_mask(img, 0.5, 0.5, 0)
            .and_then(|img| high_pass_sharpen(img, 0.3))
            .and_then(|img| clarity(img, 0.5, 1.0));
        assert!(result.is_ok());
    }
}
