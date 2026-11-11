# Letters

A desktop PDF editor for students, for Windows and macOS. Every tool is
explained in plain English, and the app is built to stay light: a 500 page
file should feel the same as a 5 page one.

Letters is early. Right now it is a fast viewer and file opener. See
[CHANGELOG.md](CHANGELOG.md) for what works today and
[ARCHITECTURE.md](ARCHITECTURE.md) for how it is put together. The design
rules are in [docs/DESIGN.md](docs/DESIGN.md).

## What you need

| | Windows | macOS |
|---|---|---|
| Rust (stable) | [rustup.rs](https://rustup.rs) | [rustup.rs](https://rustup.rs) |
| Node.js, current LTS | [nodejs.org](https://nodejs.org) | [nodejs.org](https://nodejs.org) |
| C toolchain | see below | `xcode-select --install` |
| Web view | WebView2 (already part of Windows 11) | built in |

**Windows C toolchain.** Either works:

- *Visual Studio Build Tools* with "Desktop development with C++" (needs
  administrator rights), with Rust's default `msvc` toolchain; or
- *MinGW-w64* from [winlibs.com](https://winlibs.com) unpacked anywhere and
  added to `PATH` (no administrator rights), with
  `rustup default stable-x86_64-pc-windows-gnu`.

## Build and run

From the repository root:

**Windows (PowerShell)**

```powershell
.\tools\fetch-pdfium.ps1     # once: downloads the pdf engine into vendor\pdfium
npm install                  # once: installs the build tools and the ui's packages
npm run dev                  # runs the app and reloads the ui when you edit it
```

**macOS**

```sh
sh tools/fetch-pdfium.sh
npm install
npm run dev
```

Other commands:

| Command | What it does |
|---|---|
| `npm test` | runs the Rust tests and the UI tests |
| `npm run check` | runs the linters (clippy and svelte-check) |
| `npx tauri build --no-bundle` | builds the optimised program, `target/release/letters` |

The Rust tests need the PDF engine, so run the fetch script first.
Installers and file associations are a later stage and are not set up yet.

So far this has only been built and run on Windows 11 with the MinGW
toolchain. The macOS steps are the expected ones but have not been run.

## Where things are

```
app/shell        the desktop program: windows, file opening, the bridge to the ui
app/shell-core   the parts of the shell that can be tested without a window
app/ui           the interface (Svelte + TypeScript)
core/pdf         opens pdfs and renders pages as tiles (Rust, pdfium)
docs             design notes
tools            scripts: fetching pdfium, the icon source
```

## Libraries and their licenses

| Library | Used for | License |
|---|---|---|
| [PDFium](https://pdfium.googlesource.com/pdfium/) (prebuilt by [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)) | reading and drawing PDFs | Apache-2.0 |
| [pdfium-render](https://crates.io/crates/pdfium-render) | Rust bindings to PDFium | MIT or Apache-2.0 |
| [Tauri](https://tauri.app) | windows, packaging | MIT or Apache-2.0 |
| [Svelte](https://svelte.dev), [Vite](https://vite.dev) | the interface | MIT |
| [png](https://crates.io/crates/png), [serde](https://serde.rs), [thiserror](https://crates.io/crates/thiserror) | small helpers | MIT or Apache-2.0 |

Nothing here restricts distributing the app. MuPDF, Ghostscript and Poppler
are deliberately not used, because their AGPL/GPL licenses would.
