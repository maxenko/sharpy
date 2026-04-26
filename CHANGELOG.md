# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-04-26

### Added
- `Operation::apply(image)` so each variant of the shared `Operation` enum
  can be applied directly without constructing a builder
- `SharpeningBuilder::operations(&self) -> &[Operation]` accessor for
  inspecting (and replaying) the operations a builder will run
- `Image::from_arc_rgb(Arc<RgbImage>)` constructor for the copy-on-write
  case where the caller already holds an `Arc`-shared source
- Memory bounds checking to prevent processing extremely large images
  (max 100 MP, max dimension 65 k)
- Comprehensive integration tests for image processing algorithms,
  including edge-strength measurement and visual-regression coverage
- Shared `Operation` enum between library and CLI
- GitHub Actions CI: build + test on Linux / Windows / macOS, clippy with
  `-D warnings`, and `cargo fmt --check`
- Optional `sharpy-gui` Windows desktop demo (separate workspace member,
  `publish = false`, never shipped to crates.io). Drag-drop image loading,
  live slider preview, worker-thread save

### Changed
- Repo is now a Cargo workspace; the root crate (`sharpy`) is the default
  member, so `cargo build`/`test` at the root stays lib-only and does not
  pull in GUI dependencies
- `Cargo.lock` is now tracked (the workspace ships binaries)
- `Image::from_rgb()` and `Image::from_dynamic()` return `Result<Image>`
  for safety
- `clamp_u8` is now `pub(crate)` (was private with duplicated
  reimplementations in callers)
- Optimized parallel processing to eliminate intermediate vector
  allocations
- Improved memory efficiency by processing image rows in-place
- Consolidated duplicate Operation enums between CLI and library
- CLI: `clap::ValueEnum`-derived parsing for `--method` and `--preset`
  arguments, slice-pattern `parse_single_operation` with case-insensitive
  matching

### Fixed
- Memory inefficiency in `unsharp_mask`, `enhance_edges`, and `clarity`
- Unnecessary cloning when using `Arc` in `into_arc_dynamic()`
- README overly promotional language and incorrect "zero dependencies"
  claim
- Pre-existing `bench` compile error (`Image::from_rgb` missing `?`)

### Performance
- Reduced memory usage by ~50% for large images through streaming pixel
  processing
- Eliminated collection of all pixels into vectors before applying
  changes
- Improved cache locality with row-based parallel processing

## [0.1.0] - 2025-08-03

### Added
- Initial release of sharpy image sharpening library
- Four sharpening algorithms:
  - Unsharp mask - Classic sharpening method
  - High-pass sharpening - Convolution-based enhancement
  - Edge enhancement - Using Sobel and Prewitt operators
  - Clarity - Local contrast enhancement
- Builder pattern API for chaining operations
- Six built-in presets:
  - Subtle - Light sharpening for general use
  - Moderate - Balanced sharpening with clarity
  - Strong - Heavy sharpening for soft images
  - Edge-aware - Emphasizes edges while preserving smooth areas
  - Portrait - Optimized for portraits (avoids over-sharpening skin)
  - Landscape - Maximum detail extraction for landscapes
- CLI tool with commands:
  - `sharpy unsharp` - Apply unsharp mask
  - `sharpy highpass` - Apply high-pass sharpening
  - `sharpy edges` - Enhance edges
  - `sharpy clarity` - Apply clarity enhancement
  - `sharpy preset` - Use built-in presets
  - `sharpy batch` - Process multiple files
- Parallel processing using Rayon for optimal performance
- Copy-on-write semantics for efficient memory usage
- Comprehensive documentation and examples
- Benchmarks using Criterion

### Performance
- Separable convolution for Gaussian blur operations
- Chunk-based parallel processing for better cache locality
- Zero-copy operations where possible

### Dependencies
- image 0.25
- rayon 1.10
- thiserror 2.0
- clap 4.5 (CLI only)
- indicatif 0.18 (CLI only)
- glob 0.3 (CLI only)
- anyhow 1.0 (CLI only)

[Unreleased]: https://github.com/maxenko/sharpy/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/maxenko/sharpy/compare/v0.1.0...v0.2.1
[0.1.0]: https://github.com/maxenko/sharpy/releases/tag/v0.1.0