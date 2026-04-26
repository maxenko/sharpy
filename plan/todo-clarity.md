&#x20;Optimization brief: replace clarity's O(window²) local mean with an integral image



&#x20;   Current code (D:/GitHub/sharpy/src/sharpening.rs, clarity fn)



&#x20;   For each pixel (x, y), the algorithm computes the average luminance over a (2\*radius+1)² neighborhood by iterating every pixel in the window:



&#x20;   for dy in -(half\_window as i32)..=(half\_window as i32) {

&#x20;       for dx in -(half\_window as i32)..=(half\_window as i32) {

&#x20;           let nx = (x as i32 + dx).clamp(0, width as i32 - 1) as u32;

&#x20;           let ny = (y as i32 + dy).clamp(0, height as i32 - 1) as u32;

&#x20;           local\_sum += calculate\_luminance(original.get\_pixel(nx, ny));

&#x20;           count += 1;

&#x20;       }

&#x20;   }

&#x20;   let local\_avg = local\_sum / count as f32;



&#x20;   Cost per pixel: O(window²) luminance calls. At radius=20 that's 1600 ops × every pixel. For a 24 MP image that's \~38 billion ops.



&#x20;   Replacement: summed-area table (integral image)



&#x20;   An integral image I(x, y) stores the sum of all luminance values in the rectangle from (0,0) to (x,y) inclusive. Once built, the sum over any axis-aligned rectangle \[x0..x1] × \[y0..y1] is computable in 4 lookups

&#x20;   regardless of rectangle size:



&#x20;   sum = I(x1, y1) - I(x0-1, y1) - I(x1, y0-1) + I(x0-1, y0-1)



&#x20;   So the per-pixel cost in clarity drops from O(window²) to O(1).



&#x20;   Algorithm



&#x20;   Step 1 — build the integral image (one rayon pass, two sweeps):



&#x20;   1. Allocate Vec<f64> integral of size (width+1) \* (height+1), initialized to 0. The +1 in each dimension gives a zero-padded row/column at index 0, eliminating bounds checks at the rectangle edges.

&#x20;   2. Use f64 (not f32) for the accumulator: a 65536×65536 image of all-255 luminance fits comfortably in f64 but overflows f32's precision well before the bottom-right corner.

&#x20;   3. For each (x, y) in row-major order:

&#x20;   I(x+1, y+1) = lum(x,y) + I(x, y+1) + I(x+1, y) - I(x, y)

&#x20;   3. This recurrence has a sequential dependency, but you can parallelize it as: row-prefix-sums in parallel over rows, then column-prefix-sums in parallel over columns. (rayon's par\_iter\_mut over chunks works for both

&#x20;   passes.)



&#x20;   Step 2 — rewrite the inner loop:



&#x20;   let x0 = (x as i32 - half\_window as i32).max(0) as u32;

&#x20;   let x1 = ((x as i32 + half\_window as i32) as u32).min(width - 1);

&#x20;   let y0 = (y as i32 - half\_window as i32).max(0) as u32;

&#x20;   let y1 = ((y as i32 + half\_window as i32) as u32).min(height - 1);



&#x20;   let area = ((x1 - x0 + 1) \* (y1 - y0 + 1)) as f32;

&#x20;   let sum = integral\_lookup(\&I, x1+1, y1+1)

&#x20;           - integral\_lookup(\&I, x0,   y1+1)

&#x20;           - integral\_lookup(\&I, x1+1, y0)

&#x20;           + integral\_lookup(\&I, x0,   y0);

&#x20;   let local\_avg = (sum as f32) / area;



&#x20;   Note: area is the exact pixel count in the (possibly clipped at image edges) window, so border pixels still get a correct mean — no need for the old clamping-while-counting pattern.



&#x20;   Expected speedup



&#x20;   - At radius=20 (worst case): \~50–500× faster on CPU. The new bottleneck is integral-image construction, which is O(width\*height) once.

&#x20;   - At radius=2 (typical): roughly 5–10× faster — still wins because we trade 25 luminance-function calls for 4 array indexes.

&#x20;   - Memory: one extra f64 allocation of size (w+1)\*(h+1) ≈ 8 bytes × 24 MP ≈ 192 MB for a 24 MP image. Significant but acceptable; can be released after clarity returns.



&#x20;   Behavior preservation



&#x20;   Not byte-identical to the current code. Float summation associates differently between window-sum-per-pixel and prefix-sum-then-difference, so individual pixels may differ by ±1 LSB after the as u8 cast. The existing

&#x20;   integration tests in tests/integration\_test.rs use MSE thresholds, not exact matches, so they should pass — but verify.



&#x20;   If byte-identical is required, gate the new path behind a clarity\_fast feature flag or a builder option, leaving the old path as the default. For a library that already markets itself on performance, switching the

&#x20;   default is the right call.



&#x20;   Scope



&#x20;   - Touches: src/sharpening.rs (rewrite clarity), possibly extract integral-image helper to src/utils.rs.

&#x20;   - Does not touch: public API, other algorithms, GUI crate, CLI.

&#x20;   - Test plan: existing tests should still pass with MSE tolerance. Add one new test asserting that the integral-image fast path agrees with the naive path within ±2 per channel on a 100×100 fixture.



&#x20;   Why this beats GPU as the next step



&#x20;   Same algorithmic class as the GPU win (eliminating per-pixel work), but: zero new deps, no driver/precision portability, no upload/download, single-digit lines of new code, and the speedup at typical radii is competitive

&#x20;    with a GPU dispatch's fixed overhead.

