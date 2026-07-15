# ADR 0002: Bounded JPEG tile pipeline

Status: Accepted for the Phase-0 vertical slice  
Date: 2026-07-11

## Decision

Decode JPEG files on a bounded pool of one to four native worker threads. Convert each decoded image into independent 512×512 RGBA8 tiles, then move those tiles to the UI thread without copying their pixel buffers. Upload no more than 32 MiB per frame to project-owned WGPU textures.

Render image tiles through egui_wgpu's callback interface inside eframe's existing WGPU pass. Each pane has its own transient vertex buffer and viewport transform, while GPU textures are shared by image identity. Pan and zoom therefore update small vertex buffers without rebuilding or reading back textures.

The first decoder uses the pure-Rust JPEG path exposed by the image crate, currently backed by zune-jpeg. Benchmark it against libjpeg-turbo with representative camera JPEGs before making the release-1 decoder choice permanent.

## Consequences

- A 100 MP source does not depend on GPU support for a 10,000-pixel monolithic texture.
- Decode and tiling do not block the egui event loop.
- Replacing a pane cancels or ignores stale work.
- Upload work is bounded per frame, so large files refine progressively.
- Native Windows MSVC cross-builds remain possible without packaging a C JPEG library yet.
- This full-resolution-only slice still uploads and draws every tile at fit view. A multiresolution preview/pyramid and visible-tile priority queue are required before the four-by-100 MP performance gate.
- JPEG EXIF orientation, ICC conversion, metadata, and malformed-input process isolation remain follow-up work.
