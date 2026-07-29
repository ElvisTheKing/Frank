# Frank

> *frankly, Frank is an image compare tool*

Frank is a fast, cross-platform desktop viewer for photographers and pixel peepers who want to compare fine detail, exposure, and rendering across multiple images.

## Features

- Compare one to eight images in configurable row, column, and grid layouts.
- Open JPEG and Olympus/OM System ORF RAW files by file picker, command line, context menu, or drag and drop.
- Open RAW files quickly through their embedded previews, then develop one or all at full resolution on demand, on load, or when zoom requires it.
- Use synchronized or individual zoom and pan, including persistent manual alignment and a reset option.
- Designate a reference image, highlight differing capture metadata, and use Auto align or manual alignment for translation and scale.
- View images at pixel-perfect `1:1` with sharp nearest-neighbor pixels at 100% and closer.
- Match full-resolution RAW rendering to its embedded JPEG by default, apply non-destructive GPU exposure adjustments, and normalize each currently visible view to the reference.
- Reorder, replace, or close individual panes and switch to a borderless Clean view.
- Display configurable camera, lens, exposure, resolution, quality, and user-note information for each image.
- Choose Light, Dark, or System theme and retain global interface preferences between restarts.
- Run native builds on Windows, Linux, and macOS.

## Main controls

| Control | Function |
| --- | --- |
| **Open…** | Opens up to eight images. Frank creates or removes panes to match the number selected. Images can also be dropped directly into the window. |
| **Add pane** | Adds an empty comparison pane. Use the minus button in a pane header or its right-click menu to close that specific pane. |
| **Layout** | Arranges panes automatically as rows, columns, or a compact grid. Images can be dragged to change their positions. |
| **Fit** | Fits every image inside its pane. This is the synchronized default when a comparison is opened. |
| **1:1** | Displays one source-image pixel per physical screen pixel. At 100% and above, pixels are shown without smoothing. |
| **Sync** | Links zoom and pan between panes. The adjacent mode menu controls whether synchronization is relative to fit, width, height, or source pixels. Manual offsets and scale differences are preserved until **Reset alignment** is used. |
| **Align** | Sets the reference pane and aligns the active pane—or all panes—to it. Auto align estimates translation and scale from image features; manual alignment uses two matching points selected in each image. |
| **RAW** | Controls full-resolution RAW development. Develop the active RAW, develop every loaded RAW, develop automatically on load, or re-match an active RAW to its embedded preview. |
| **Exposure** | Applies an instant GPU exposure adjustment to the active or linked panes. **Normalize visible views to reference** samples only the currently visible areas and balances them against the reference pane. |
| **Clean view** | Hides pane headers and controls for a borderless comparison while retaining the compact image titles. |
| **…** | Selects the theme and chooses which camera and capture fields appear in pane titles. |

## RAW workflow

Frank first displays a RAW file's embedded JPEG so large comparisons open quickly. The pane still uses the full RAW dimensions for zoom calculations. When the preview no longer contains enough detail, Frank switches to the developed full-resolution image; **Develop on load** performs that work immediately instead.

Full RAW development is limited to two simultaneous jobs to keep memory use bounded. **Develop all RAWs** queues every loaded RAW that has not already been developed, while completed results are reused rather than developed again.

By default, the developed RAW is tone-matched to its own embedded JPEG. This match, manual exposure, and viewport normalization are non-destructive GPU adjustments, so changing them does not repeat RAW development.

## Alignment and navigation

Choose a reference pane from the **Align** menu or an image's right-click menu. **Auto align** is intended for different captures of the same subject and adjusts translation and scale. If the images differ too much for automatic feature matching, manual alignment lets you click two corresponding points in the reference and target.

You can also align images by eye: disable **Sync**, adjust each pane independently, then enable it again. Frank retains those relative pan and zoom differences so the subject stays aligned during synchronized navigation. Alignment can be reset for the active pane or for the entire comparison.

Right-clicking an image provides the common pane actions in one place: set it as reference, align it, replace or close the image, close the pane, fit the image, or switch it to `1:1`.
