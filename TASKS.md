# ImageCompareTool development tasks

This is the persistent product backlog. Performance work is resumed when an acceptance gate fails, not merely because another micro-optimization is possible.

## Current feature work

- [x] Drag pane title bars to reorder images in the comparison grid.
- [x] Toggle synchronization membership per pane.
- [x] Open/replace images through a native multi-file picker.
- [x] Edit and display a short user note with each image.
- [x] Configure which RAW metadata fields appear in pane titles.
- [x] Add/remove panes and choose common grid layouts.
- [x] Report zoom as physical pixel magnification, where 100% is one source pixel per framebuffer pixel.
- [x] Preserve free-mode per-pane pan/zoom registration when synchronized navigation is re-enabled, with reset.
- [x] Use nearest-neighbor sampling at 100% and closer magnification.
- [x] Apply a compact comparison-first visual design with clipped two-row headers and one-pixel pane separators.
- [x] Use full RAW coordinates while showing embedded previews and develop automatically past preview-native zoom.
- [x] Add develop-on-load, exact batch pane sizing, per-pane close controls, borderless controls hiding, and image context menus.
- [x] Deduplicate completed/pending RAW development and allow two bounded full RAW developments concurrently.
- [x] Reorganize the toolbar into workspace, navigation, processing, and presentation groups with Clean view mode.
- [x] Provide Light, Dark, and System themes, with native system-theme detection and a Dark fallback.
- [x] Persist versioned global UI preferences across restarts without restoring comparison sessions.

## Release 1

- [x] Develop the active RAW pane on demand for source-resolution 1:1 display.
- [x] Add versioned `RawRecipe` diagnostics and validate the current pipeline against the OM-5 corpus.
- [x] Implement the documented reference RAW exposure/view-transform pipeline after diagnostics pass.
- [ ] Collect and validate Canon EOS R6 RAW and C-RAW fixtures, including highlight-priority modes.
- [x] Apply JPEG EXIF orientation and embedded ICC-to-sRGB conversion.
- [ ] Extract JPEG capture metadata for configurable pane titles.
- [ ] Add keyboard shortcuts and a discoverable help overlay.
- [ ] Package and test Windows, Linux, and macOS releases.

## Deferred format backlog

- [ ] Add TIFF and BigTIFF loading after collecting representative compressed, tiled, high-bit-depth, and color-profile fixtures.

## Deferred workflow backlog

- [ ] Save and restore comparison sessions, including image paths, ordering, layout, notes, title configuration, sync settings, zoom, and pan.

## Deferred performance backlog

Current checkpoint: four approximately 80.6 MP OM-5 ORFs open through 3200×2400 embedded previews with a repeated Windows startup peak of 444–445 MiB and settled memory of approximately 155–160 MiB. See [the benchmark report](docs/benchmark-om5-preview-2026-07-11.md).

- [ ] Generate multiresolution levels and upload only visible tiles.
- [ ] Put CPU tiles, staging buffers, GPU textures, and caches under one memory budget.
- [ ] Decode JPEG previews directly into tiles where practical.
- [ ] Prioritize visible tiles and cancel obsolete uploads after viewport changes.
- [ ] Add a disk-backed pyramid/cache with versioned keys and eviction.
- [ ] Benchmark true 100 MP sources and define p50/p95 latency gates.
- [ ] Move full RAW development into an isolated helper process with strict memory limits and a disk-backed multiresolution pyramid.
- [ ] Benchmark the active-pane full RAW path across the supported camera corpus and document its neutral recipe.
- [ ] Profile integrated/discrete GPUs on Windows, Vulkan Linux, and Apple Silicon.

## Release 2 research

- [x] Normalize RAW and JPEG panes over their shared visible region, rejecting clipped extremes and applying an instantaneous per-pane GPU exposure layer.
- [x] Add manual per-pane GPU exposure with optional linked-pane synchronization and reset controls.
- [x] Match a RAW rendering to its own embedded JPEG preview using a separate GPU exposure layer.
- [ ] Upgrade exposure matching to aligned overlapping samples after automatic registration is available.
- [x] Add optional preview-matched full-resolution RAW rendering using a bounded global tone curve.
- [ ] Add camera-specific baseline exposure policies only when backed by metadata documentation and fixtures.
- [x] Add an expert linear RAW diagnostic mode.
- [ ] Feature/point extraction and automatic pan/zoom matching.
- [ ] Manual registration correction.
- [ ] Basic exposure, brightness, contrast, and color adjustments.

## Release 3 research

- [ ] Browser/WASM host with a deliberately bounded format and memory feature set.
