# GPU Acceleration — Future Considerations

Notes from a design discussion about adding optional GPU support to the
sharpy library. Captured here so the reasoning isn't lost if/when this
gets revisited.

**Status:** Not planned for 0.2.x. Worth considering for a 0.3 or later
release if the GUI ever needs to feel butter-smooth on very large
images (40+ MP) or batch mode becomes a frequent workload.

## Goal

Add optional GPU acceleration to the four sharpening algorithms, with two
hard constraints:

1. **Zero setup on the user's end.** No CUDA Toolkit install, no OpenCL
   runtime, no separate SDK download. The crate works out of the box.
2. **Truly optional.** Users who don't enable GPU pay zero cost — no
   extra dependencies, no extra compile time, no extra binary size.

## Recommended Tool: `wgpu`

`wgpu` is the right pick because it satisfies both constraints:

- **Pure-Rust crate.** No system runtime to install. wgpu picks the best
  backend at runtime (Vulkan on Linux, Metal on macOS, DX12 on Windows,
  WebGPU in browsers).
- **Driver shims bundled.** Modern Rust toolchains link the Vulkan loader
  / DX12 / Metal bindings statically. End users don't install anything
  extra; the binary just works.
- **Compute shaders via WGSL.** The four sharpening kernels (unsharp,
  high-pass, edge enhance, clarity) are straightforward to translate.

### Rejected alternatives

| Option | Why not |
|---|---|
| **CUDA** | NVIDIA-only, requires CUDA Toolkit install. Fails the zero-setup constraint. |
| **OpenCL** | Cross-vendor but the runtime isn't always preinstalled (especially on Windows). Half-zero-setup. |
| **Vulkan / DX12 / Metal directly** | Platform-specific. wgpu wraps these for free. |
| **CPU SIMD** | Already exploited via rayon parallelism; not what GPU support means. |

## Tradeoffs (Why It's Not Free)

Even with wgpu, GPU support is a meaningful cost:

1. **Compile time and binary size.** wgpu pulls in ~100+ transitive crates
   and adds 5–10 MB to the final binary. This is why it must live behind
   a Cargo feature flag, not be an always-on dependency.

2. **Per-call overhead.** wgpu init is ~50–200 ms; GPU↔CPU memory transfer
   adds further per-image cost. For one-shot small images, the CPU path
   is faster end-to-end.

3. **Each algorithm needs a WGSL compute shader.** Most of the four kernels
   port cleanly. `clarity` is the trickiest because its window size is
   variable and the local-mean reduction needs care to avoid bandwidth
   stalls. (See also `plan/todo-clarity.md` — the integral-image rewrite
   would also help GPU performance.)

4. **CPU fallback is non-negotiable.** Headless servers, broken drivers,
   sandboxed CI, very old hardware — all need a working code path. The
   feature flag would build BOTH paths and pick at runtime, falling back
   silently if wgpu init fails.

## When GPU Actually Wins

After the overhead, GPU only beats CPU+rayon in three specific scenarios.
Two of these we've already built:

### 1. The GUI's live slider drag (strongest case)

Init is paid once at app startup. The source image stays GPU-resident
across hundreds of preview re-runs. Each slider tick reuses the upload,
runs the shader, reads back the result. This is exactly the use case GPU
was designed for, and where users would feel the difference: smooth
60 fps preview vs. perceptible lag at 24 MP today.

### 2. Batch mode (`sharpy batch`)

Init amortized across N images. Upload-of-(N+1) can be pipelined with
compute-of-N. Useful when batches are large enough (say N > 4) for the
amortization to matter.

### 3. Heavy parameters on single images

`clarity` at radius=20 on a 24 MP source takes 30s–2min on CPU
(O(window²) per pixel). GPU could do this in ~1s. Init overhead vanishes
against minutes of compute. This applies even to single-image CLI runs
when parameters are punishing enough.

## When GPU Loses

For a CLI doing one image at default parameters: ~150 ms wasted on init
plus transfer, to save maybe 50 ms of compute. Net loss. The CPU+rayon
path is faster end-to-end.

## Integration Strategy

If we ever do this, the integration shape would be:

- **Library:** new optional Cargo feature `gpu` (or `wgpu`). New module
  `src/gpu/` with shaders + dispatch. Public API gets a runtime
  `try_gpu_first` toggle, but the default stays CPU. CPU fallback is
  always built.
- **GUI:** opts in to GPU at startup. Init cost paid during the splash
  / first frame. All slider previews go through GPU.
- **CLI single ops:** stay CPU. Not worth the overhead.
- **CLI `batch`:** opts in to GPU when N exceeds some threshold (say 4).
  Below that, CPU is faster.
- **Feature flag in `sharpy-gui/Cargo.toml`:** `sharpy = { path = "..", features = ["gpu"] }`.

## Out of Scope (For This Discussion)

- WebAssembly target via WebGPU. Possible with wgpu but adds a separate
  set of constraints (browser-only file I/O, no rayon, etc.). Worth its
  own plan if ever pursued.
- GPU-resident pipeline composition (run all four stages without
  intermediate GPU↔CPU roundtrips). The plumbing is non-trivial; only
  worth it once the basic GPU path is stable and benchmarked.
- Auto-tuning the CPU/GPU crossover threshold. A static heuristic (image
  size + parameter cost) is fine for v1.

## Verdict

For 0.2.x: don't do it. The CPU+rayon path is fast enough for everything
except interactive GUI preview on huge sources, and that's a niche
workload. The added compile time, binary size, and maintenance burden
outweigh the user-facing benefit at the current scale.

Revisit when:

- The GUI demo gets used on > 40 MP sources frequently and slider lag is
  a real complaint.
- Batch mode becomes a primary workflow (current usage seems
  single-image-dominant).
- Someone offers a concrete need that the existing CPU path can't meet.
