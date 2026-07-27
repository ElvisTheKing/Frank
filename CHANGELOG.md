# Changelog

All notable changes to ImageCompareTool are documented in this file.

The project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Run cross-platform builds and release publication only when a version tag is pushed.

## [0.2.0] - 2026-07-28

This is the first non-test tagged release of ImageCompareTool.

### Added

- Added automatic full-resolution RAW development when zoom exceeds the embedded preview's native detail, plus an optional develop-on-load setting.
- Added exact one-to-eight-pane sizing for opened image batches, per-pane close and replace actions, image context menus, and Clean view for borderless comparison.
- Added Light, Dark, and System themes with native system-theme detection and a Dark fallback.
- Added versioned persistence for global UI, RAW, synchronization, layout, and title-field preferences without restoring comparison sessions.

### Changed

- Reorganized the toolbar into clearer workspace, navigation, processing, and presentation groups.
- Deduplicated completed and pending full-resolution RAW requests while allowing up to two memory-bounded RAW developments concurrently.
- Split the desktop application into focused modules for application coordination, comparison math, pane runtime state, preferences, RAW policy, and batch workspace changes.
- Included the changelog in Windows, Linux, and macOS CI packages.

### Fixed

- Prevented `1:1` and repeated identical recipe requests from unnecessarily redeveloping a RAW image.
- Limited RAW-only actions to RAW panes and kept RAW preview matching as a non-destructive GPU display adjustment.
- Marked Windows release builds as GUI applications so they no longer open an additional console window.

[Unreleased]: https://github.com/ElvisTheKing/Frank/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ElvisTheKing/Frank/compare/v0.1.0-test.2...v0.2.0
