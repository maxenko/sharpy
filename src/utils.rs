use image::{Rgb, RgbImage};
use rayon::prelude::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeMethod {
    Sobel,
    Prewitt,
}

const KERNEL_SIZE_3: usize = 3;

pub const HIGH_PASS_KERNEL: [f32; 9] = [0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0];

const SOBEL_X: [f32; 9] = [-1.0, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];

const SOBEL_Y: [f32; 9] = [-1.0, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];

const PREWITT_X: [f32; 9] = [-1.0, 0.0, 1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0];

const PREWITT_Y: [f32; 9] = [-1.0, -1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

#[inline]
pub(crate) fn clamp_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[inline]
fn clamp_coord(c: i32, max: u32) -> u32 {
    c.clamp(0, max as i32 - 1) as u32
}

pub fn calculate_luminance(pixel: &Rgb<u8>) -> f32 {
    0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32
}

/// Applies Gaussian blur using a separable 1-D kernel (two passes).
pub fn gaussian_blur(img: &RgbImage, radius: f32) -> RgbImage {
    let kernel_size = (radius * 6.0).ceil() as usize | 1;
    let kernel = Arc::new(generate_gaussian_kernel(kernel_size, radius));
    let half = kernel_size / 2;

    let temp = blur_pass(img, &kernel, half, BlurAxis::Horizontal);
    blur_pass(&temp, &kernel, half, BlurAxis::Vertical)
}

#[derive(Clone, Copy)]
enum BlurAxis {
    Horizontal,
    Vertical,
}

fn blur_pass(src: &RgbImage, kernel: &[f32], half: usize, axis: BlurAxis) -> RgbImage {
    let (width, height) = src.dimensions();
    let mut out = RgbImage::new(width, height);

    out.enumerate_rows_mut().par_bridge().for_each(|(y, row)| {
        for (x, _, pixel) in row {
            let mut sums = [0.0_f32; 3];
            let mut weight_sum = 0.0;

            for (k, &weight) in kernel.iter().enumerate() {
                let offset = k as i32 - half as i32;
                let (sx, sy) = match axis {
                    BlurAxis::Horizontal => (clamp_coord(x as i32 + offset, width), y),
                    BlurAxis::Vertical => (x, clamp_coord(y as i32 + offset, height)),
                };
                let sp = src.get_pixel(sx, sy);
                sums[0] += sp[0] as f32 * weight;
                sums[1] += sp[1] as f32 * weight;
                sums[2] += sp[2] as f32 * weight;
                weight_sum += weight;
            }

            *pixel = Rgb([
                clamp_u8(sums[0] / weight_sum),
                clamp_u8(sums[1] / weight_sum),
                clamp_u8(sums[2] / weight_sum),
            ]);
        }
    });

    out
}

fn generate_gaussian_kernel(size: usize, sigma: f32) -> Vec<f32> {
    let half_size = size / 2;
    let two_sigma_sq = 2.0 * sigma * sigma;

    let mut kernel: Vec<f32> = (0..size)
        .map(|i| {
            let x = i as f32 - half_size as f32;
            (-x * x / two_sigma_sq).exp()
        })
        .collect();

    let sum: f32 = kernel.iter().sum();
    for value in &mut kernel {
        *value /= sum;
    }
    kernel
}

/// Applies a square convolution kernel to an image.
pub fn apply_convolution(img: &RgbImage, kernel: &[f32], kernel_size: usize) -> RgbImage {
    let (width, height) = img.dimensions();
    let half = kernel_size / 2;
    let mut out = RgbImage::new(width, height);

    out.enumerate_rows_mut().par_bridge().for_each(|(y, row)| {
        for (x, _, pixel) in row {
            let mut sums = [0.0_f32; 3];

            for (ky, kernel_row) in kernel.chunks_exact(kernel_size).enumerate() {
                let sy = clamp_coord(y as i32 + ky as i32 - half as i32, height);
                for (kx, &weight) in kernel_row.iter().enumerate() {
                    let sx = clamp_coord(x as i32 + kx as i32 - half as i32, width);
                    let sp = img.get_pixel(sx, sy);
                    sums[0] += sp[0] as f32 * weight;
                    sums[1] += sp[1] as f32 * weight;
                    sums[2] += sp[2] as f32 * weight;
                }
            }

            *pixel = Rgb([clamp_u8(sums[0]), clamp_u8(sums[1]), clamp_u8(sums[2])]);
        }
    });

    out
}

/// Linearly blends `processed` over `original` by `strength` (clamped to [0, 1]).
pub fn blend_images(original: &RgbImage, processed: &RgbImage, strength: f32) -> RgbImage {
    let (width, height) = original.dimensions();
    let blend = strength.clamp(0.0, 1.0);
    let inv_blend = 1.0 - blend;

    let mut out = RgbImage::new(width, height);

    out.enumerate_rows_mut().par_bridge().for_each(|(y, row)| {
        for (x, _, pixel) in row {
            let op = original.get_pixel(x, y);
            let pp = processed.get_pixel(x, y);
            *pixel = Rgb([
                clamp_u8(op[0] as f32 * inv_blend + pp[0] as f32 * blend),
                clamp_u8(op[1] as f32 * inv_blend + pp[1] as f32 * blend),
                clamp_u8(op[2] as f32 * inv_blend + pp[2] as f32 * blend),
            ]);
        }
    });

    out
}

/// Applies edge detection by combining horizontal and vertical kernels.
pub fn apply_edge_detection(img: &RgbImage, method: EdgeMethod) -> RgbImage {
    let (kx, ky) = match method {
        EdgeMethod::Sobel => (&SOBEL_X, &SOBEL_Y),
        EdgeMethod::Prewitt => (&PREWITT_X, &PREWITT_Y),
    };

    let x_edges = apply_convolution(img, kx, KERNEL_SIZE_3);
    let y_edges = apply_convolution(img, ky, KERNEL_SIZE_3);

    let (width, height) = img.dimensions();
    let mut out = RgbImage::new(width, height);

    out.enumerate_rows_mut().par_bridge().for_each(|(y, row)| {
        for (x, _, pixel) in row {
            let xm = calculate_luminance(x_edges.get_pixel(x, y));
            let ym = calculate_luminance(y_edges.get_pixel(x, y));
            let magnitude = (xm * xm + ym * ym).sqrt().clamp(0.0, 255.0) as u8;
            *pixel = Rgb([magnitude, magnitude, magnitude]);
        }
    });

    out
}
