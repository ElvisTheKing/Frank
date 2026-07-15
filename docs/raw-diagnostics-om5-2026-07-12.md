# OM-5 RAW diagnostics — 2026-07-12

The version 1 diagnostic recipe was run over all eight local ORF fixtures. The current recipe is `rawler-0.7/default`, camera white balance, zero baseline/automatic/comparison EV, Rawler's default highlight behavior, and an sRGB transfer without a dedicated scene-to-display tone mapper.

| File | Size | Black / white | Display p50 / p99 | Display-clipped pixels | Preview-to-RAW median |
|---|---:|---:|---:|---:|---:|
| P3083007 | 10368×7776 | 256 / 4000 | .380 / .596 | 0% | +0.63 EV |
| P4283497 | 8160×6120 | 256 / 4000 | .137 / .984 | 0.864% | +1.23 EV |
| P7175880 | 10368×7776 | 256 / 4000 | .341 / .392 | 0% | +0.62 EV |
| P7175881 | 10368×7776 | 256 / 4000 | .329 / .482 | 0% | +0.61 EV |
| P7175961 | 10368×7776 | 257 / 4000 | .173 / .914 | 0.403% | +0.16 EV |
| P7175962 | 10368×7776 | 257 / 4000 | .208 / .651 | 0.150% | +0.36 EV |
| P7175963 | 10368×7776 | 257 / 4000 | .204 / .529 | 0.083% | +0.34 EV |
| P9047038 | 8160×6120 | 256 / 4000 | .275 / 1.000 | 4.735% | +0.39 EV |

These are display-output clipping measurements after the current development, not sensor mosaic clipping measurements. They nevertheless expose why a fixed brightness gain is unsafe: `P9047038` already has 4.7% of output pixels with at least one channel at 255, while the preview/RAW median gap varies from +0.16 to +1.23 EV.

The next pipeline step must retain a high-precision intermediate and measure clipping before 8-bit conversion. Automatic exposure should be coupled to a soft highlight shoulder instead of multiplying the existing RGBA8 output.
