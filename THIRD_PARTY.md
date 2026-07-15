# Third-party notices

ImageCompareTool is distributed under AGPL-3.0-or-later, but its dependencies retain their own licenses. `Cargo.lock` is the authoritative version inventory for a build.

## Rawler 0.7.2

- Component: Rawler, part of the DNGLab project
- Purpose: camera RAW identification and metadata decoding
- Source: <https://github.com/dnglab/dnglab>
- License: GNU Lesser General Public License, version 2.1 (LGPL-2.1)
- License text: <https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html>

Distributors must retain the Rawler copyright and license notices and meet the LGPL's applicable source-code, modification, reverse-engineering, and relinking requirements. The project should automate a complete dependency-license bundle before its first public binary release. This notice is an engineering record, not legal advice.

## rfd 0.17.2

- Component: Rusty File Dialogs
- Purpose: native desktop multi-file picker
- Source: <https://github.com/PolyMeilex/rfd>
- License: MIT
- License text: <https://github.com/PolyMeilex/rfd/blob/master/LICENSE>

The Linux build uses rfd's XDG Desktop Portal backend rather than its GTK development-library backend. A supported desktop portal or Zenity must be present at runtime for Linux file dialogs.

## moxcms 0.8.1

- Component: moxcms
- Purpose: pure-Rust embedded ICC profile conversion to sRGB
- Source: <https://github.com/awxkee/moxcms>
- License: BSD-3-Clause OR Apache-2.0
- License text: <https://github.com/awxkee/moxcms/tree/master/LICENSE.md>
