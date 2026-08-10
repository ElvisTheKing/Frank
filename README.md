# Frank

> *frankly, Frank is an image compare tool*

Frank is a fast, cross-platform desktop viewer for photographers and pixel peepers who want to compare fine detail, exposure, and rendering across multiple images.

## Features

- Compare one to eight images in configurable row, column, and grid layouts.
- Open JPEG and Olympus/OM System ORF RAW files by file picker, command line, context menu, or drag and drop.
- Open RAW files quickly through their embedded previews, then develop one or all at full resolution on demand, on load, or when zoom requires it.
- Use synchronized or individual zoom and pan, including persistent manual alignment and a reset option.
- Designate a reference image, highlight differing capture metadata, and use Auto align or manual alignment for translation and scale.
- In the maximized view, blink the reference with `Space` or compare both images with a draggable vertical or horizontal split.
- Maximize a pane into the full workspace for a larger Blink or Split inspection, then return to the unchanged grid.
- View images at pixel-perfect `1:1` with sharp nearest-neighbor pixels at 100% and closer.
- Match full-resolution RAW rendering to its embedded JPEG by default, apply non-destructive GPU exposure adjustments, and normalize each currently visible view to the reference.
- Preview relativistic redshift while receding or blueshift while approaching, with a mirrored exponential speed control reaching 99.99% of light speed in either direction.
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
| **Maximize / All panes** | Use the maximize icon in a pane header, double-click its image, or press `F` to fill the workspace without removing other panes. The focused toolbar exposes Target, Reference, left/right Split, and top/bottom Split directly. Use **All panes**, `F`, `Esc`, or double-click again to restore the grid. |
| **Fit** | Fits every image inside its pane. This is the synchronized default when a comparison is opened. |
| **1:1** | Displays one source-image pixel per physical screen pixel. At 100% and above, pixels are shown without smoothing. |
| **Sync** | Links zoom and pan between panes. The adjacent mode menu controls whether synchronization is relative to fit, width, height, or source pixels. Manual offsets and scale differences are preserved until **Reset alignment** is used. |
| **Align** | Sets the reference pane and aligns the active pane—or all panes—to it. Click `REF` in a pane header or press `R` to make the active pane the reference. Auto align estimates translation and scale from image features; manual alignment uses two matching points selected in each image. |
| **RAW** | Controls full-resolution RAW development. Develop the active RAW, develop every loaded RAW, develop automatically on load, or re-match an active RAW to its embedded preview. |
| **Exposure** | Applies an instant GPU exposure adjustment to the active or linked panes. **Normalize visible views to reference** samples only the currently visible areas and balances them against the reference pane. |
| **Doppler** | Applies a shared, non-destructive spectral Doppler preview to every pane. Move left from rest for approaching/blueshift or right for receding/redshift; the mirrored exponential scale reaches 99.99% of light speed in either direction. |
| **Clean view** | Hides pane headers and controls for a borderless comparison while retaining the compact image titles. |
| **…** | Selects the theme, chooses which camera and capture fields appear in pane titles, and enables a source-pixel grid at 600% magnification and closer. |

## Relativistic Doppler-shift preview

Open **Doppler**, enable the preview, and set the signed radial speed. Frank uses the longitudinal relativistic Doppler factor `sqrt((1 + β) / (1 - β))`, where `β = v/c`: positive velocity increases separation and produces redshift, while negative velocity means approach and produces blueshift. The menu reports the direction, speed, wavelength multiplier, shift `z`, and a live 500 nm example. Light speed itself is excluded because the factor becomes singular at `+c` and physical observers cannot reach either limit; the slider range is `-0.9999 c` to `+0.9999 c`.

The preview reconstructs a smooth spectrum from linear sRGB, shifts it in wavelength, and integrates the result through CIE 1931 color matching functions on the GPU display path. Zero speed is calibrated to reproduce the original pixels exactly. Bands that move beyond the visible range disappear naturally rather than being wrapped around as a hue rotation.

An RGB photograph contains only three visible-light measurements, not its original spectrum or any ultraviolet/infrared data, so no RGB-based simulator can determine the shifted color uniquely. Frank's wavelength mapping is relativistically exact, while its reconstructed spectrum is a documented approximation. At extreme speeds, recorded visible bands shift out of view and the missing UV or IR bands cannot replace them. The preview also does not simulate field-of-view aberration or absolute radiometric brightening/dimming.

## RAW workflow

Frank first displays a RAW file's embedded JPEG so large comparisons open quickly. The pane still uses the full RAW dimensions for zoom calculations. When the preview no longer contains enough detail, Frank switches to the developed full-resolution image; **Develop on load** performs that work immediately instead.

Full RAW development is limited to two simultaneous jobs to keep memory use bounded. **Develop all RAWs** queues every loaded RAW that has not already been developed, while completed results are reused rather than developed again.

By default, the developed RAW is tone-matched to its own embedded JPEG. This match, manual exposure, and viewport normalization are non-destructive GPU adjustments, so changing them does not repeat RAW development.

## Alignment and navigation

Choose a reference by clicking `REF` in its pane header, pressing `R` while it is active, using the **Align** menu, or using the image's right-click menu. The reference keeps a subtle blue border, including in Clean view. **Auto align** is intended for different captures of the same subject and adjusts translation and scale. If the images differ too much for automatic feature matching, manual alignment lets you click two corresponding points in the reference and target.

The **Align** menu reports feature count, candidate matches, geometric inliers, confidence, and median error for the latest automatic attempt. **Show match diagnostics** overlays accepted matches in green and rejected candidates in orange, making a weak or failed alignment easier to diagnose.

You can also align images by eye: disable **Sync**, adjust each pane independently, then enable it again. Frank retains those relative pan and zoom differences so the subject stays aligned during synchronized navigation. Alignment can be reset for the active pane or for the entire comparison.

Reference Blink and Split are deliberately limited to the maximized view, so selecting panes in the grid always shows their own images. Select a loaded non-reference pane, then use its header maximize icon, double-click the image, or press `F`. The focused toolbar keeps Target, Reference, left/right Split, and top/bottom Split visible as one-click modes. Hold `Space` for a temporary reference blink. Drag the Split divider directly; click **Center** or double-click the divider to reset it.

**All panes**, `F`, `Esc`, or another image double-click returns to the unchanged grid. The image context menu can open either Split orientation directly in the maximized view.

Right-clicking an image provides the common pane actions in one place: set it as reference, align it, replace or close the image, close the pane, fit the image, or switch it to `1:1`.
