# ImageCompareTool

ImageCompareTool is a performance-first, cross-platform image comparison viewer for photographers and pixel peepers.

The project is in its Phase-0 architecture spike. The current vertical slice establishes:

- a portable viewer/session model;
- an egui UI crate with a dynamic two-to-eight-pane workspace shell;
- background JPEG and Olympus/OM System ORF-preview decoding with stale-request cancellation;
- 512×512 CPU and GPU image tiles;
- validated OM-5 ORF preview-offset lookup with a bounded metadata-scan fallback;
- weighted decode admission held through GPU upload under a 160 MiB working budget;
- bounded per-frame upload and project-owned WGPU pane callbacks;
- synchronized pan and pointer-anchored zoom;
- per-pane `SYNC`/`FREE` membership for individual navigation;
- drag-to-reorder comparison panes by their title bars;
- native multi-file Open dialog that sizes the workspace to the selected image count (one to eight), plus per-pane replacement from the context menu;
- editable short image notes shown persistently on the second pane-title line;
- live-configurable pane titles with camera, lens, resolution, bit depth, ISO, shutter, aperture, focal length, and preview status;
- add/remove pane controls and Auto, row, column, two-column, and three-column layouts;
- four explicit full-resolution RAW recipes: As Shot, Auto Reference, Preview Matched, and Linear Diagnostic;
- bounded linear RAW exposure with a hue-preserving soft highlight shoulder;
- full-frame linear-light normalization of RAW and JPEG panes using instantaneous GPU uniforms;
- RAW-to-embedded-preview matching as a separate non-destructive GPU adjustment;
- JPEG EXIF orientation and embedded ICC-profile conversion to sRGB;
- manual per-pane GPU exposure with optional linked-pane synchronization;
- viewport-aware RAW/JPEG normalization with clipped-extreme rejection and confidence reporting;
- physical zoom percentages where 100% means one image pixel per framebuffer pixel;
- persistent per-pane pan/zoom registration captured from free mode and preserved during synchronized navigation;
- nearest-neighbor image sampling at 100% and closer for sharp pixel boundaries;
- a compact comparison-first interface with two-row clipped pane headers, clearly separated workspace/navigation/processing/presentation toolbar groups, Clean view, and one-pixel pane separators;
- optional borderless comparison with pane controls hidden and compact title overlays;
- Light, Dark, and system-matched themes selectable from the View menu, with System as the default and a Dark fallback when detection is unavailable;
- versioned global preferences that survive restarts without reopening the previous comparison session;
- an eframe desktop host forced to the `wgpu` backend;
- workspace tests and cross-platform CI.

The initial performance acceptance case is four simultaneous 100 MP images on the baseline machine documented in [DESIGN.md](DESIGN.md). Baseline JPEG display works, and OM-5 ORF files open through their embedded previews with source dimensions and camera metadata clearly identified. **View 1:1** develops the active RAW pane at full source resolution while retaining the preview until the replacement tiles are ready. JPEG EXIF/ICC metadata, TIFF/BigTIFF, Canon EOS R6 CR3 validation, and moving full RAW development to an isolated helper process remain follow-up work.

## Try the vertical slice

Build the Windows executable as described below, then use **Open…**, pass JPEG or supported RAW paths on the command line, or drag them into the running window:

    dist\windows-x64\imagecompare-desktop.exe photo-a.jpg photo-b.jpg

For RAW files, navigation and zoom always use the full RAW dimensions even while the embedded JPEG is displayed. Development starts automatically when zoom passes the preview's native resolution; **Develop RAWs on load** can request it immediately instead. **View 1:1** also reaches full source detail and does not redevelop a completed or pending full-resolution RAW. The RAW selector offers As Shot, Auto Reference, and Linear Diagnostic recipes; explicitly selecting a different recipe redevelops, while an identical request is deduplicated. **Match Preview** compares the active RAW rendering with its retained embedded-JPEG statistics and applies a GPU exposure offset. **Normalize** works on both RAW and JPEG panes relative to the active pane. These display adjustments update a per-pane GPU uniform and do not redevelop or re-upload an image. The decode budget permits at most two simultaneous full RAW developments to improve batch latency while bounding peak memory pressure.

Global interface choices are saved automatically every five seconds and when the application closes. This includes the theme, Clean view, pixel grid, RAW-on-load and RAW recipe choices, synchronization settings, layout, and configured title fields. Open images, notes, pane ordering, and pan/zoom remain session-only; restoring complete comparison sessions is intentionally still in the backlog.

To generate a deterministic 12 MP JPEG for smoke or performance testing:

    docker compose run --rm dev cargo run -p image-loader --example generate_jpeg -- dist/test-pattern.jpg 4096 3072

## Docker development environment

Docker Desktop is the canonical local build environment; Rust and the native Linux build dependencies do not need to be installed on the host. The toolchain is pinned to Rust 1.92.0, the minimum supported by egui/eframe 0.35. Build the reusable environment once:

```text
docker compose build dev
```

Run the full local quality gate:

```text
docker compose run --rm dev cargo fmt --all --check
docker compose run --rm dev cargo check --workspace --all-targets
docker compose run --rm dev cargo test --workspace
docker compose run --rm dev cargo clippy --workspace --all-targets -- -D warnings
```

A Linux release binary can be produced with:

```text
docker compose run --rm dev cargo build --release -p imagecompare-desktop
```

Compiled Linux output lives in the Docker-managed `linux-target` volume, keeping the host checkout clean. Use `docker compose run --rm dev bash` for an interactive shell. Running the Linux desktop GUI itself requires a graphical Linux session. Windows can be cross-built as described below; native CI runners still validate every supported OS, and macOS packaging requires a macOS runner.

### Windows MSVC build in Docker

The `windows-msvc` service uses pinned `cargo-xwin` 0.23.0 to cross-compile an x64 Windows executable with the MSVC ABI from Docker's Linux engine:

```text
docker compose build windows-msvc
docker compose run --rm windows-msvc
```

This downloads Microsoft CRT and Windows SDK files into the Docker-managed `xwin-cache` volume and keeps cross-compiled artifacts in the separate `windows-target` volume; using `cargo-xwin` indicates acceptance of the linked [Microsoft license](https://go.microsoft.com/fwlink/?LinkId=2086102). The build script copies the result to the host-visible `dist/windows-x64/imagecompare-desktop.exe`. Native execution and GPU validation still happen on Windows, and macOS packages still require a macOS runner.

## Architecture rule

The desktop shell must not own decoding, viewport mathematics, cache policy, or renderer scene state. Native-only fast paths belong behind portable interfaces so a capability-limited browser host can be explored for version 3.

Architecture decisions are recorded under [docs/adr](docs/adr).
Feature work and deferred optimization tasks are tracked in [TASKS.md](TASKS.md).
Release history is recorded in [CHANGELOG.md](CHANGELOG.md).

The local OM-5 corpus result is recorded in [the RAW preview benchmark](docs/benchmark-om5-preview-2026-07-11.md).

## GitHub Actions builds

The `ci` workflow runs formatting, checks, tests, Clippy, and an optimized native build on Windows, Linux, and macOS for every push and pull request. It can also be started manually with **Run workflow**. Successful runs provide downloadable artifacts:

- `imagecompare-windows-*`: ZIP containing `imagecompare-desktop.exe`;
- `imagecompare-linux-*`: compressed tarball containing the native executable;
- `imagecompare-macos-*`: ZIP containing `ImageCompareTool.app`.

These CI artifacts are unsigned development builds. Public macOS distribution still requires Apple code signing and notarization; Windows signing can be added separately when a certificate is available.

## License

ImageCompareTool is licensed under the GNU Affero General Public License, version 3 or later (`AGPL-3.0-or-later`). See [LICENSE](LICENSE). Dependency license notices are recorded in [THIRD_PARTY.md](THIRD_PARTY.md).
