# RAW development research and proposed display contract

Status: design proposal, 2026-07-12. No rendering behavior is changed by this document.

## Problem observed

The current full-resolution RAW path uses `RawDevelop::default()` from Rawler and converts its sRGB result to RGBA8. This performs black/white rescaling, demosaicing, crop/orientation, camera white balance, camera calibration, and an sRGB conversion, but it does not reproduce the camera maker's proprietary JPEG rendering.

On two OM-5 samples, a local preview-versus-full-RAW luminance probe showed:

| File | Embedded preview mean / median | Current RAW mean / median | Median difference |
|---|---:|---:|---:|
| `P3083007.ORF` | 0.565 / 0.588 | 0.382 / 0.380 | about 0.63 EV |
| `P7175961.ORF` | 0.250 / 0.192 | 0.204 / 0.173 | about 0.16 EV |

The unequal difference demonstrates that a fixed gain is not a general correction. Much of the visible difference is the camera JPEG's exposure policy, tone curve, highlight compression, color profile, and creative settings rather than a failure to apply sRGB gamma.

## Findings

### Exposure and tone mapping are separate operations

Exposure should scale linear, scene-referred data and establish a useful middle-gray level. A later view transform should compress scene dynamic range into the display range. Applying only an sRGB transfer function does not perform that dynamic-range mapping.

darktable's scene-referred workflow performs most operations in linear RGB, adjusts mid-tones with exposure, and then maps scene black/white to the display with filmic or sigmoid. Its documented historical default included a modest +0.5 EV starting boost, while noting that some cameras need a larger camera-specific preset. This is evidence for a configurable baseline, not a universal +2 EV correction.

### Automatic exposure must preserve highlights

Histogram-based auto exposure is a useful starting point, but it is not ground truth. RawTherapee's Auto Levels adjusts several coupled controls, including exposure, highlight compression/reconstruction, black, brightness, and contrast. Its clipping tolerance is explicit. LibRaw likewise separates histogram auto-brightening, linear exposure shift, and highlight preservation.

Therefore a simple mean or median match to the embedded JPEG is unsuitable as the reference algorithm. It can over-brighten night/high-key images, reproduce a camera creative style unintentionally, and clip information which exists in the RAW.

### Metadata can intentionally explain a dark RAW

Camera dynamic-range and highlight-priority modes often protect highlights by lowering the sensor exposure and compensating during the maker's JPEG development. RapidRAW issue #711 identified this as a concrete cause of apparently dark CR3 files. The issue discussion points to vendor-specific tags and darktable's handling of Fuji dynamic-range metadata. DNG provides standardized baseline-exposure metadata, but ORF/CR3 maker-note policies require camera-specific validation.

We should preserve the original EXIF exposure-compensation value as metadata, while treating any maker rendering compensation as a separate baseline-development value. They must not be silently conflated.

### Highlight handling must precede destructive clipping

RapidRAW issues #246 and #419 show two failure modes relevant to us: Olympus/Panasonic clipped channels becoming purple, and recoverable highlights being flattened to white. RapidRAW initially hardcoded a compression point, then made the clipping point adjustable because camera behavior differs. ImageCompareTool should avoid an arbitrary post-demosaic clamp and retain a high-precision, unbounded intermediate until the view transform.

### Camera-JPEG matching is useful but is not neutral

RawTherapee's auto-matched tone curve copies the in-camera tone response curve from the embedded JPEG. This is a valuable convenience view, but it introduces a nonlinear rendering look. Exact camera matching is usually impossible without the maker's profile, tone curve, local processing, noise reduction, sharpening, and lens corrections.

For a pixel-peeping comparison tool, this needs an explicit name and must not masquerade as the reference RAW result.

## Proposed user-visible modes

1. **Embedded preview** — the camera-created JPEG, decoded exactly as stored. Fast and useful for composition and a camera-look reference.
2. **Reference RAW** — the default source-resolution mode. Deterministic, scene-referred development with camera WB/profile, metadata baseline compensation where validated, conservative automatic middle-gray placement, and a highlight-preserving display transform. No default sharpening, denoising, local tone mapping, or creative color style.
3. **Preview-matched RAW** — later opt-in mode. Develop at source resolution, then estimate a bounded global exposure/tone mapping from the embedded preview. Label it as an approximation and expose the estimated adjustment. Never use local matching because it would alter pixel-level contrast.
4. **Linear diagnostic** — later expert mode for debugging profiles, clipping, and decoder behavior. Not intended to look pleasing.

The pane title/status should always show which mode is active. Synchronized comparisons should use the same development recipe/version unless the user explicitly unlocks processing synchronization.

## Proposed reference pipeline

Keep a floating-point or 16-bit-plus intermediate through the view transform:

1. Decode mosaic and metadata.
2. Subtract per-channel black levels and normalize against reliable sensor white levels.
3. Correct bad pixels only where decoder metadata supports it.
4. Apply a highlight reconstruction strategy before channel clipping.
5. Demosaic.
6. Apply camera white balance as the initial neutral estimate.
7. Convert with a validated camera matrix/profile to a linear working RGB space.
8. Apply standardized or validated maker-note baseline exposure in linear light.
9. Estimate a conservative display exposure from robust luminance percentiles, with explicit clipping allowance and bounded EV correction.
10. Apply a monotonic, hue-stable sigmoid/filmic view transform with a soft highlight shoulder.
11. Convert to the display/output color space and apply its transfer function.
12. Quantize only for GPU upload/cache output.

Record a `RawRecipe` beside every result: recipe version, decoder version, WB source, camera profile identity, baseline EV source/value, automatic EV value, tone-map parameters, and highlight method. This makes comparisons reproducible and invalidates cached tiles correctly when the pipeline changes.

## Acceptance criteria before changing the default

- Do not clip any channel before highlight reconstruction and the final view transform.
- A gray-card fixture maps consistently to the documented middle-gray target.
- Auto exposure has a documented percentile/clipping policy and a bounded adjustment range.
- Rendering the same RAW and recipe is deterministic across supported hosts within a small numeric tolerance.
- OM-5 and Canon R6 fixtures cover daylight, tungsten, high ISO, deep shadows, specular highlights, and camera highlight-priority/dynamic-range modes.
- Compare against the embedded preview, but do not require identical histograms or color.
- Export a diagnostic report containing sensor black/white levels, channel maxima, WB multipliers, profile, baseline EV, auto EV, and clipped-pixel counts.
- 1:1 output receives no implicit sharpening, denoising, or geometric correction.

## Staged implementation recommendation

### Stage A — instrumentation first

Add a `RawRecipe` and diagnostics without changing pixels. Extend the probe to report linear percentiles, per-channel maxima/clipping, metadata compensation candidates, and the exact Rawler stages used. Validate the eight OM-5 files and collect several R6 CR3/C-RAW fixtures.

### Stage B — reference rendering

Retain a high-precision intermediate; add explicit linear exposure and a conservative sigmoid/filmic view transform. Initially use a documented fixed baseline plus bounded percentile auto exposure. Keep both values visible and independently disableable. Add highlight reconstruction or, until it exists, warn when a channel clips rather than pretending it was recovered.

### Stage C — metadata policies

Implement standardized DNG `BaselineExposure`/`BaselineExposureOffset`, then individually tested maker-note policies for OM System/Olympus and Canon. Store every policy as data keyed by camera/mode and cover it with fixtures; do not infer undocumented compensation silently.

### Stage D — preview-matched option

Fit only a monotonic global luminance curve using a downscaled full-RAW render and embedded JPEG. Bound exposure and shoulder behavior, reject unreliable matches, and display the resulting EV/curve. Keep it optional.

## Decision

Do not fix the reported darkness with a fixed +2 to +3 EV gain or direct preview-histogram matching. Implement Stage A next, then make **Reference RAW** the source-resolution default after Stage B passes the acceptance corpus. Preserve the embedded JPEG as the immediate camera-look reference and add preview matching later.

## Sources

- [darktable scene-referred workflow](https://docs.darktable.org/usermanual/3.6/en/overview/workflow/edit-scene-referred/)
- [darktable color pipeline](https://docs.darktable.org/usermanual/4.2/en/special-topics/color-pipeline/)
- [darktable filmic RGB](https://docs.darktable.org/usermanual/4.6/en/module-reference/processing-modules/filmic-rgb/)
- [RawTherapee toolchain pipeline](https://rawpedia.rawtherapee.com/Toolchain_Pipeline)
- [RawTherapee exposure and highlight reconstruction](https://rawpedia.rawtherapee.com/Exposure)
- [LibRaw output/development parameters](https://www.libraw.org/docs/API-datastruct.html)
- [Adobe DNG specification 1.7.1](https://helpx.adobe.com/content/dam/help/en/photoshop/pdf/DNG_Spec_1_7_1_0.pdf)
- [RapidRAW #711: CR3 files appear very dark](https://github.com/CyberTimon/RapidRAW/issues/711)
- [RapidRAW #968: CR2 darker than JPEG](https://github.com/CyberTimon/RapidRAW/issues/968)
- [RapidRAW #246: Olympus clipped highlights turn purple](https://github.com/CyberTimon/RapidRAW/issues/246)
- [RapidRAW #419: highlights clip too quickly](https://github.com/CyberTimon/RapidRAW/issues/419)
- [darktable PR #19347: Fuji DR metadata compensation](https://github.com/darktable-org/darktable/pull/19347)

