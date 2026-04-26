use crate::utils::EdgeMethod;
use crate::{Image, Operation, Result};

/// Builder for configuring and applying sharpening operations.
///
/// Provides a fluent interface for complex sharpening workflows.
///
/// # Example
/// ```no_run
/// use sharpy::{Image, SharpeningBuilder};
///
/// let result = Image::load("input.jpg")
///     .unwrap()
///     .sharpen()
///     .unsharp_mask(2.0, 1.5, 10)
///     .clarity(0.5, 3.0)
///     .apply()
///     .unwrap();
/// ```
pub struct SharpeningBuilder {
    image: Image,
    operations: Vec<Operation>,
}

impl SharpeningBuilder {
    pub(crate) fn new(image: Image) -> Self {
        Self {
            image,
            operations: Vec::new(),
        }
    }

    /// Adds unsharp mask operation to the pipeline.
    pub fn unsharp_mask(mut self, radius: f32, amount: f32, threshold: u8) -> Self {
        self.operations.push(Operation::UnsharpMask {
            radius,
            amount,
            threshold,
        });
        self
    }

    /// Adds high-pass sharpening to the pipeline.
    pub fn high_pass(mut self, strength: f32) -> Self {
        self.operations
            .push(Operation::HighPassSharpen { strength });
        self
    }

    /// Adds edge enhancement to the pipeline.
    pub fn edge_enhance(mut self, strength: f32, method: EdgeMethod) -> Self {
        self.operations
            .push(Operation::EnhanceEdges { strength, method });
        self
    }

    /// Adds clarity enhancement to the pipeline.
    pub fn clarity(mut self, strength: f32, radius: f32) -> Self {
        self.operations
            .push(Operation::Clarity { strength, radius });
        self
    }

    /// Applies all configured operations and returns the result.
    pub fn apply(self) -> Result<Image> {
        self.operations
            .iter()
            .try_fold(self.image, |img, op| op.apply(img))
    }

    /// Returns the number of operations in the pipeline.
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Returns the operations queued in this builder, in execution order.
    ///
    /// Useful for testing/inspecting that a built-in preset (or hand-built
    /// pipeline) has the expected operations without running them.
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Clears all operations from the pipeline.
    pub fn clear(mut self) -> Self {
        self.operations.clear();
        self
    }
}

/// Preset sharpening configurations for common use cases.
pub struct SharpeningPresets;

impl SharpeningPresets {
    /// Subtle sharpening suitable for most images.
    pub fn subtle(image: Image) -> SharpeningBuilder {
        SharpeningBuilder::new(image).unsharp_mask(0.8, 0.6, 2)
    }

    /// Moderate sharpening for slightly soft images.
    pub fn moderate(image: Image) -> SharpeningBuilder {
        SharpeningBuilder::new(image)
            .unsharp_mask(1.0, 1.0, 3)
            .clarity(0.3, 2.0)
    }

    /// Strong sharpening for very soft images.
    pub fn strong(image: Image) -> SharpeningBuilder {
        SharpeningBuilder::new(image)
            .unsharp_mask(1.5, 1.5, 2)
            .high_pass(0.3)
            .clarity(0.5, 3.0)
    }

    /// Edge-focused sharpening that preserves smooth areas.
    pub fn edge_aware(image: Image) -> SharpeningBuilder {
        SharpeningBuilder::new(image)
            .edge_enhance(0.8, EdgeMethod::Sobel)
            .unsharp_mask(0.5, 0.8, 5)
    }

    /// Portrait sharpening that avoids over-sharpening skin.
    pub fn portrait(image: Image) -> SharpeningBuilder {
        SharpeningBuilder::new(image)
            .unsharp_mask(1.2, 0.7, 10)
            .clarity(0.2, 5.0)
    }

    /// Landscape sharpening for maximum detail.
    pub fn landscape(image: Image) -> SharpeningBuilder {
        SharpeningBuilder::new(image)
            .unsharp_mask(1.0, 1.2, 1)
            .edge_enhance(0.5, EdgeMethod::Sobel)
            .clarity(0.4, 3.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    fn create_test_image() -> Image {
        Image::from_rgb(RgbImage::new(100, 100)).unwrap()
    }

    #[test]
    fn test_builder_single_operation() {
        let img = create_test_image();
        let builder = img.sharpen().unsharp_mask(1.0, 1.0, 0);
        assert_eq!(builder.operation_count(), 1);
        assert!(builder.apply().is_ok());
    }

    #[test]
    fn test_builder_multiple_operations() {
        let img = create_test_image();
        let builder = img
            .sharpen()
            .unsharp_mask(1.0, 1.0, 0)
            .high_pass(0.5)
            .clarity(0.5, 2.0);
        assert_eq!(builder.operation_count(), 3);
        assert!(builder.apply().is_ok());
    }

    #[test]
    fn test_operations_accessor() {
        let img = create_test_image();
        let builder = img.sharpen().unsharp_mask(1.0, 0.8, 2).clarity(0.5, 3.0);
        let ops = builder.operations();
        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], Operation::UnsharpMask { .. }));
        assert!(matches!(ops[1], Operation::Clarity { .. }));
    }

    #[test]
    fn test_presets() {
        let img = create_test_image();
        assert!(SharpeningPresets::subtle(img.clone()).apply().is_ok());
        assert!(SharpeningPresets::moderate(img.clone()).apply().is_ok());
        assert!(SharpeningPresets::strong(img.clone()).apply().is_ok());
        assert!(SharpeningPresets::edge_aware(img.clone()).apply().is_ok());
        assert!(SharpeningPresets::portrait(img.clone()).apply().is_ok());
        assert!(SharpeningPresets::landscape(img).apply().is_ok());
    }
}
