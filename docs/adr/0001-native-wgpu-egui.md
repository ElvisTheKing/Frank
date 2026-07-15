# ADR 0001: Native Rust, wgpu, and egui boundaries

Status: Accepted for Phase 0  
Date: 2026-07-11

## Decision

Use Rust throughout. The Phase-0 desktop shell uses eframe with its `wgpu` renderer because it supplies tested window/event integration and direct access to the underlying WGPU render state. Image tiles will be rendered by a project-owned custom WGPU callback, not by uploading whole images as ordinary egui widgets.

Keep viewer state, UI layout, renderer scene state, and native services in separate crates. If measurement shows eframe overhead or lifecycle constraints, replace only the desktop host with direct `winit`; do not rewrite the image engine.

## Consequences

- Vulkan, DX12, and Metal share WGSL and scene code.
- UI and image rendering can use one device, queue, and surface.
- There is no webview, IPC, base64 transfer, or GPU readback for presentation.
- A future WASM host can reuse the model, egui UI, and compatible renderer code.
- Phase 0 must benchmark the custom paint callback before this host choice becomes permanent.

