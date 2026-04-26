//! # Sharpy
//!
//! High-performance image sharpening library for Rust.
//!
//! This library provides multiple sharpening algorithms optimized for performance
//! with parallel processing support. It includes both a library API and a CLI tool.
//!
//! ## Quick Start
//!
//! ```no_run
//! use sharpy::Image;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load and sharpen an image
//! let image = Image::load("photo.jpg")?;
//! let sharpened = image.unsharp_mask(1.0, 1.0, 0)?;
//! sharpened.save("photo_sharp.jpg")?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Available Algorithms
//!
//! - **Unsharp Mask** - Classic sharpening by subtracting a blurred version
//! - **High-Pass** - Convolution-based sharpening using a high-pass kernel
//! - **Edge Enhancement** - Detects and enhances edges using Sobel or Prewitt
//! - **Clarity** - Local contrast enhancement for improved detail
//!
//! ## Using the Builder Pattern
//!
//! ```no_run
//! use sharpy::{Image, EdgeMethod};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let result = Image::load("landscape.jpg")?
//!     .sharpen()
//!     .unsharp_mask(1.0, 1.2, 1)
//!     .edge_enhance(0.5, EdgeMethod::Sobel)
//!     .clarity(0.4, 3.0)
//!     .apply()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Performance
//!
//! All algorithms use parallel processing via Rayon for optimal performance.
//! The library uses copy-on-write semantics to minimize memory allocations.

use image::{DynamicImage, RgbImage};
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Hard cap on total pixels (~100 MP) to keep allocations bounded.
const MAX_IMAGE_PIXELS: usize = 100_000_000;
/// Hard cap on either dimension (65 536 px).
const MAX_IMAGE_DIMENSION: u32 = 65536;

mod builder;
mod operations;
mod sharpening;
mod utils;

pub use builder::{SharpeningBuilder, SharpeningPresets};
pub use operations::Operation;
pub use utils::EdgeMethod;

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("Invalid dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("Invalid parameter: {param} = {value}")]
    InvalidParameter { param: String, value: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Image format error: {0}")]
    Format(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, ImageError>;

#[derive(Clone)]
enum ImageData {
    Owned(RgbImage),
    Shared(Arc<RgbImage>),
}

impl ImageData {
    fn get_mut(&mut self) -> &mut RgbImage {
        match self {
            ImageData::Owned(img) => img,
            ImageData::Shared(arc_img) => {
                *self = ImageData::Owned((**arc_img).clone());
                if let ImageData::Owned(img) = self {
                    img
                } else {
                    unreachable!()
                }
            }
        }
    }

    fn get_ref(&self) -> &RgbImage {
        match self {
            ImageData::Owned(img) => img,
            ImageData::Shared(arc_img) => arc_img,
        }
    }
}

fn invalid_param(param: &str, value: impl ToString) -> ImageError {
    ImageError::InvalidParameter {
        param: param.to_string(),
        value: value.to_string(),
    }
}

/// Validates that `value` lies in `(0, max]` (rejects 0 and negatives).
fn ensure_positive_max(param: &str, value: f32, max: f32) -> Result<()> {
    if value <= 0.0 || value > max {
        return Err(invalid_param(param, value));
    }
    Ok(())
}

/// Validates that `value` lies in `[0, max]` (accepts 0).
fn ensure_non_negative_max(param: &str, value: f32, max: f32) -> Result<()> {
    if value < 0.0 || value > max {
        return Err(invalid_param(param, value));
    }
    Ok(())
}

/// The main image type that provides sharpening operations.
///
/// This struct uses copy-on-write semantics internally to minimize memory
/// allocations when cloning images.
///
/// # Examples
///
/// ```no_run
/// use sharpy::Image;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Load from file
/// let image = Image::load("photo.jpg")?;
///
/// // Create from existing image data
/// use image::RgbImage;
/// let rgb_image = RgbImage::new(800, 600);
/// let image = Image::from_rgb(rgb_image)?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Image {
    data: ImageData,
}

impl Image {
    pub fn from_dynamic(img: DynamicImage) -> Result<Self> {
        Self::validate_dimensions(img.width(), img.height())?;
        Ok(Self::from_dynamic_unchecked(img))
    }

    pub fn from_rgb(img: RgbImage) -> Result<Self> {
        let (width, height) = img.dimensions();
        Self::validate_dimensions(width, height)?;
        Ok(Self::from_rgb_unchecked(img))
    }

    fn from_dynamic_unchecked(img: DynamicImage) -> Self {
        Self {
            data: ImageData::Owned(img.to_rgb8()),
        }
    }

    fn from_rgb_unchecked(img: RgbImage) -> Self {
        Self {
            data: ImageData::Owned(img),
        }
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_dynamic(image::open(path)?)
    }

    fn validate_dimensions(width: u32, height: u32) -> Result<()> {
        if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        let total_pixels = width as usize * height as usize;
        if total_pixels > MAX_IMAGE_PIXELS {
            return Err(ImageError::InvalidDimensions { width, height });
        }

        Ok(())
    }

    pub fn from_arc_dynamic(arc_img: Arc<DynamicImage>) -> Result<Self> {
        let (width, height) = (arc_img.width(), arc_img.height());
        Self::validate_dimensions(width, height)?;

        match Arc::try_unwrap(arc_img) {
            Ok(img) => Ok(Self::from_dynamic_unchecked(img)),
            Err(arc_img) => Ok(Self {
                data: ImageData::Shared(Arc::new(arc_img.to_rgb8())),
            }),
        }
    }

    pub fn from_arc_rgb(arc_img: Arc<RgbImage>) -> Result<Self> {
        let (width, height) = arc_img.dimensions();
        Self::validate_dimensions(width, height)?;

        match Arc::try_unwrap(arc_img) {
            Ok(img) => Ok(Self::from_rgb_unchecked(img)),
            Err(arc_img) => Ok(Self {
                data: ImageData::Shared(arc_img),
            }),
        }
    }

    pub fn from_dynamic_ref(img: &DynamicImage) -> Result<Self> {
        Self::validate_dimensions(img.width(), img.height())?;
        Ok(Self {
            data: ImageData::Owned(img.to_rgb8()),
        })
    }

    pub fn into_arc_dynamic(self) -> Arc<DynamicImage> {
        match self.data {
            ImageData::Owned(img) => Arc::new(DynamicImage::ImageRgb8(img)),
            // Try to claim ownership of the inner RgbImage to avoid a clone when
            // we hold the only Arc reference.
            ImageData::Shared(arc_img) => match Arc::try_unwrap(arc_img) {
                Ok(img) => Arc::new(DynamicImage::ImageRgb8(img)),
                Err(arc_img) => Arc::new(DynamicImage::ImageRgb8((*arc_img).clone())),
            },
        }
    }

    pub fn into_dynamic(self) -> DynamicImage {
        DynamicImage::ImageRgb8(self.into_rgb())
    }

    pub fn into_rgb(self) -> RgbImage {
        match self.data {
            ImageData::Owned(img) => img,
            ImageData::Shared(arc_img) => Arc::try_unwrap(arc_img).unwrap_or_else(|a| (*a).clone()),
        }
    }

    pub fn save<P: AsRef<Path>>(self, path: P) -> Result<()> {
        self.into_dynamic().save(path)?;
        Ok(())
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.data.get_ref().dimensions()
    }

    pub fn histogram(&self) -> [u32; 256] {
        let bins: [AtomicU32; 256] = std::array::from_fn(|_| AtomicU32::new(0));
        let img = self.data.get_ref();

        img.pixels().par_bridge().for_each(|pixel| {
            let luminance = utils::calculate_luminance(pixel) as usize;
            bins[luminance.min(255)].fetch_add(1, Ordering::Relaxed);
        });

        std::array::from_fn(|i| bins[i].load(Ordering::Relaxed))
    }

    pub fn unsharp_mask(self, radius: f32, amount: f32, threshold: u8) -> Result<Self> {
        ensure_positive_max("radius", radius, 10.0)?;
        ensure_non_negative_max("amount", amount, 5.0)?;
        sharpening::unsharp_mask(self, radius, amount, threshold)
    }

    pub fn high_pass_sharpen(self, strength: f32) -> Result<Self> {
        ensure_positive_max("strength", strength, 3.0)?;
        sharpening::high_pass_sharpen(self, strength)
    }

    pub fn enhance_edges(self, strength: f32, method: EdgeMethod) -> Result<Self> {
        ensure_positive_max("strength", strength, 3.0)?;
        sharpening::enhance_edges(self, strength, method)
    }

    pub fn clarity(self, strength: f32, radius: f32) -> Result<Self> {
        ensure_positive_max("strength", strength, 3.0)?;
        ensure_positive_max("radius", radius, 20.0)?;
        sharpening::clarity(self, strength, radius)
    }

    /// Creates a sharpening builder for fluent configuration.
    ///
    /// # Example
    /// ```no_run
    /// # use sharpy::Image;
    /// # let image = Image::from_rgb(image::RgbImage::new(100, 100)).unwrap();
    /// let sharpened = image.sharpen()
    ///     .unsharp_mask(1.0, 1.0, 0)
    ///     .clarity(0.5, 2.0)
    ///     .apply()
    ///     .unwrap();
    /// ```
    pub fn sharpen(self) -> SharpeningBuilder {
        SharpeningBuilder::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_creation() {
        let img = RgbImage::new(100, 100);
        let sharpy_img = Image::from_rgb(img).unwrap();
        assert_eq!(sharpy_img.dimensions(), (100, 100));
    }

    #[test]
    fn test_parameter_validation() {
        let img1 = RgbImage::new(100, 100);
        let sharpy_img1 = Image::from_rgb(img1).unwrap();
        assert!(sharpy_img1.unsharp_mask(-1.0, 1.0, 0).is_err());

        let img2 = RgbImage::new(100, 100);
        let sharpy_img2 = Image::from_rgb(img2).unwrap();
        assert!(sharpy_img2.high_pass_sharpen(-1.0).is_err());

        let img3 = RgbImage::new(100, 100);
        let sharpy_img3 = Image::from_rgb(img3).unwrap();
        assert!(sharpy_img3.enhance_edges(-1.0, EdgeMethod::Sobel).is_err());

        let img4 = RgbImage::new(100, 100);
        let sharpy_img4 = Image::from_rgb(img4).unwrap();
        assert!(sharpy_img4.clarity(-1.0, 1.0).is_err());
    }
}
