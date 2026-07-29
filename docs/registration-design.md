# Image alignment and metadata comparison

## Scope

Frank uses one loaded pane as the comparison reference. The reference is identified by a `REF` badge and can be changed from the Align menu or an image context menu. The interface uses “alignment” and “Auto align”; “registration” remains the technical name for the underlying mapping.

The first alignment release supports:

- highlighting title metadata that differs from the reference;
- manual two-point alignment;
- automatic translation and uniform-scale alignment;
- preserving the result while synchronized zooming and panning;
- resetting one target or every alignment.

Rotation is measured during manual registration and reported to the user, but is not applied. Perspective, lens-distortion, and non-rigid corrections are also out of scope. These transformations would change the meaning of pixel-level comparison and need a separate, opt-in design.

## Metadata differences

Every configured metadata value is compared with the reference pane's corresponding value. A target value gets a `Δ` prefix when it differs. Missing target data is explicit, for example `Δ ISO —`. The reference title is never marked as different from itself.

This keeps differences visible in the existing always-on title without adding a large metadata panel. It also works in Clean view, where the compact title is painted over the image.

## Registration model

Each target pane stores an affine normalized-center mapping relative to the reference:

```text
target center = reference center × center scale + center offset
target zoom   = synchronized base zoom × zoom ratio
```

The center scale is stored separately on each axis because normalized coordinates depend on image dimensions. This is important when comparing a crop with a full frame or images with different aspect ratios. The actual image transformation remains translation plus uniform scale; the two-axis center mapping only converts between the two normalized coordinate systems.

Any linked pane can initiate navigation. Frank converts that pane back to reference coordinates, then applies every target's mapping. The registered subject therefore stays in place while the user pans or zooms from either pane.

## Manual alignment workflow

The active non-reference pane is the target. Frank requests four clicks in this order:

1. point 1 in the reference;
2. the same point in the target;
3. point 2 in the reference;
4. the same point in the target.

The points should be visually distinct and far apart. The first pair establishes translation and the distance between pairs establishes scale. Users can pan and zoom between clicks. Selected points are drawn above the images and the pane awaiting the next click gets an amber outline.

If the two points are coincident or too close, registration is rejected without changing the viewport. A measured rotation of at least one degree is reported as a warning because this version does not rotate the image.

## Auto align

The decoder produces an aspect-preserving, contrast-normalized grayscale registration image with a maximum long edge of 640 pixels. An embedded RAW preview supplies it immediately; if that preview is smaller, a later full RAW decode upgrades it. Automatic registration clones this bounded representation and works on a background thread, so it never copies the full-resolution image or triggers RAW redevelopment.

The matcher:

1. detects repeatable corners at four image-pyramid scales;
2. describes local structure with exposure-resistant binary intensity tests;
3. keeps mutual nearest descriptor matches that pass an ambiguity-ratio check;
4. fits a translation, rotation, and uniform-scale similarity model from pairs of matches;
5. refines the model over its geometric inliers;
6. rejects results with too few inliers, insufficient spatial spread, excessive residual error, or unsupported rotation.

Only translation and uniform scale are applied by the current viewport model. A small fitted rotation is reported as part of registration diagnostics but is not resampled into the displayed image. Large rotations are rejected. The scale search supports substantial focal-length changes; acceptance is driven by feature consensus rather than a fixed overlap window.

Automatic results carry the source image identities, and results are discarded if either pane was replaced while matching. The status line reports confidence, inlier count, median geometric error, scale, and center offset so weak results are visible rather than silently trusted.

## Acceptance checks

- A target title marks changed and missing configured metadata relative to `REF`.
- Manual points produce the expected translation and scale for images with different dimensions.
- Registered centers and zoom ratios remain aligned when navigation starts in either linked pane.
- Flat images do not return an automatic match.
- Synthetic translated/scaled/exposure-shifted patterns are recovered within bounded error, including a 2.3× focal-length difference.
- An unrelated textured image does not return a false registration.
- Automatic matching never blocks the interface and stale results cannot alter a replaced image.
- Reset active affects only that target; reset all removes every registration adjustment.
