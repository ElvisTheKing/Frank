# OM-5 embedded-preview baseline — 2026-07-11

## Corpus

The private, Git-ignored corpus contains eight OM Digital Solutions OM-5 ORF files totaling approximately 465 MiB:

- six files: 10400×7792 sensor data, 10368×7776 crop (approximately 80.6 MP);
- two files: 8200×6132 sensor data, 8160×6120 crop (approximately 49.9 MP);
- all eight: 16-bit metadata and normal orientation;
- seven identify an Olympus M.Zuiko Digital ED 12-45mm F4.0 Pro lens; one has no resolved lens name.

Every file contains a decodable 3200×2400 JPEG preview. The original in-memory candidate validation and selection took approximately 1.4–2.1 ms per file after copying the complete ORF into memory; that excluded file I/O and caused a high concurrent startup peak.

The first optimized path read the root ORF strip offset and scanned only the approximately 1.5 MiB metadata region before sensor data. On the Docker-to-Windows bind mount this file-backed discovery took 29.5–41.9 ms per file, including access to that prefix.

All eight fixtures also contain a validated Olympus preview descriptor at byte 11,132. It points to a JPEG beginning at byte 52,224, with lengths from 944,011 to 1,238,411 bytes. Direct descriptor lookup, bounds validation, and JPEG dimension validation took 15.2–29.4 ms per file in the same environment. If that descriptor is missing or invalid, the loader falls back to the bounded metadata scan. Neither optimized path retains a source-file-sized byte buffer. Metadata parsing, full JPEG decode, tiling, and GPU upload remain outside these discovery measurements.

## Windows four-file smoke test

Build: Windows x64 MSVC release, cross-compiled in the project's Docker environment.  
Input: four approximately 80.6 MP ORFs passed on the command line.  
Observation window: 15 seconds on the baseline Windows host.

| Result | Initial whole-file path | Bounded ORF path | Direct + end-to-end budget |
| --- | ---: | ---: | ---: |
| Process remained running/responsive | yes | yes | yes (2/2 runs) |
| Working set after 15 seconds | 153.6 MiB | 154.7 MiB | 154.7–160.0 MiB |
| Peak working set during startup | 973.3 MiB | 455.1 MiB | 444.0–445.0 MiB |
| Process CPU time | 0.88 s | 0.59 s | 0.88–1.84 s |
| stderr output | none | none | none |

The bounded result uses a 160 MiB weighted decode-admission budget. An embedded-preview job reserves an estimated 80 MiB, permitting two concurrent jobs; very large JPEG estimates can reserve the entire budget and run alone. In the final column, that reservation remains active while a decoded result waits for the UI and until the renderer confirms its GPU upload is complete.

This validates the embedded-preview path, metadata display, four-pane loading, and WGPU desktop startup. It does not establish final color accuracy or the final 4×100 MP memory target.

## Follow-up performance work

1. Decode an embedded preview directly into tiles where the codec permits it.
2. Add multiresolution levels and prioritize only visible tiles for upload and drawing.
3. Track GPU allocations alongside CPU reservations under a unified cache budget.
4. Develop full RAW in an isolated helper process and build a disk-backed multiresolution pyramid.
5. Repeat with Canon EOS R6 RAW and C-RAW fixtures and with a true 100 MP-class source.

## Full RAW development probe

Rawler's default development pipeline was exercised through the same loader in a single exclusive job:

| Input | Developed size | Final RGBA tile payload | Development + tiling |
| --- | ---: | ---: | ---: |
| P4283497.ORF (approximately 49.9 MP) | 8160×6120 | 190.5 MiB | 2.16 s |
| P3083007.ORF (approximately 80.6 MP) | 10368×7776 | 307.5 MiB | 3.27 s |

These are release-Docker measurements and exclude GPU upload. Full RAW remains active-pane-only and exclusive until a helper process and multiresolution cache exist.
