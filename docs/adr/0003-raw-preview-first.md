# ADR 0003: RAW preview first

Status: Accepted for the Phase-0 vertical slice  
Date: 2026-07-11

## Decision

Open supported camera RAW files in two explicit stages. The interactive first stage extracts and decodes the largest valid embedded JPEG, then uses Rawler 0.7.2 for sensor dimensions, crop, orientation, camera, lens, and bit-depth metadata. The UI must label this result as an embedded preview and must not describe preview-pixel 1:1 as source-pixel 1:1.

Rawler's generic preview API does not expose the preview in the current OM-5 corpus. For OM-5 ORF, the vertical slice first validates the Olympus `0100` preview descriptor, its file bounds, and the referenced JPEG dimensions. If the descriptor is unavailable or invalid, it reads the root TIFF strip offset and scans only the metadata region before sensor data for structurally valid JPEG streams. Other supported RAW formats currently use a format-agnostic full-file streaming fallback. All paths are covered by synthetic tests.

Full development is an explicit second stage, requested by **View 1:1** for the active RAW pane. It uses Rawler's default pipeline: scaling, demosaic, active/default crop, camera white balance, calibration, and sRGB conversion. The preview remains visible until full-resolution tiles replace it. Full development has exclusive loader admission so previews cannot develop concurrently with it.

This is an in-process capability milestone, not the final hardening boundary. Move development into a helper process, define a versioned neutral recipe, add a multiresolution cache, and enforce memory limits before treating broad camera support as release-ready.

## Consequences

- Large OM-5 files become visible quickly without allocating full 16-bit or floating-point developed images.
- File titles can immediately show useful source and camera metadata.
- Fit, zoom, and pan operate on the preview until full development replaces the active pane; the quality indicator makes that transition visible.
- Known OM-5 ORFs use direct, validated preview offset and length fields. ORF fallback discovery reads only the metadata region before the raw strip and does not retain a complete source-file copy. Unknown RAW containers use a bounded-memory streaming fallback.
- Decode workers share a 160 MiB weighted admission budget. Two 80 MiB preview estimates can proceed concurrently; an estimate at or above the capacity runs alone.
- Reservations travel with completed decode results and remain active through renderer upload completion, preventing queued CPU tile payloads from bypassing admission.
- The four-ORF Windows startup peak fell from 973.3 MiB to 444–445 MiB after direct discovery and end-to-end admission. GPU allocations and persistent cache memory still need unified budgeting before the 4×100 MP release gate.
- Full RAW is intentionally limited to one active-pane request. An 80.6 MP OM-5 develops to 10368×7776 RGBA tiles (approximately 307.5 MiB of final CPU tiles) in the current release Docker benchmark; Rawler's floating-point intermediates require materially more transient memory.
- Malformed third-party decoder input still shares the desktop process in this spike. The planned helper-process boundary remains a release requirement.
- Rawler is LGPL-2.1 licensed. Binary distribution must preserve its notices and satisfy the applicable source/relinking requirements; see `THIRD_PARTY.md`.
