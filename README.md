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

So far this has only been built and run on Windows 11 with the MinGW
toolchain. The macOS steps are the expected ones but have not been run.

## Packaging

The installers come from `npx tauri build`, run in `app/shell`. Before it,
fill in the shipped resources once:

```powershell
.\tools\fetch-pdfium.ps1            # the pdf engine (vendor\pdfium)
.\tools\fetch-tesseract.ps1         # text recognition (vendor\tesseract)
.\tools\fetch-fonts.ps1             # bundled open fonts (vendor\fonts)
powershell -ExecutionPolicy Bypass -File tools\icon\make-document-icons.ps1
node tools\licenses\make-third-party-licenses.mjs   # THIRD-PARTY-LICENSES.txt, and what needs review
```

| System | Command | Result |
|---|---|---|
| Windows | `cd app\shell; npx tauri build` | `target\release\bundle\nsis\Letters_0.1.0_x64-setup.exe`: a per-user installer (no administrator rights) that registers Letters for PDF, Word, OpenDocument, RTF and picture files and lists it in Settings > Apps > Default apps |
| macOS | `cd app/shell && npx tauri build` | `target/release/bundle/macos/Letters.app` and `target/release/bundle/dmg/Letters_0.1.0_aarch64.dmg`, with the same document types in the app's Info.plist |

Tauri's bundler downloads NSIS (Windows) the first time. The file types,
icons, installer mode and macOS settings are in `app/shell/tauri.conf.json`
(`bundle`); `app/shell/installer-hooks.nsh` adds the document icons and the
Default Apps entry on Windows.

**Making Letters the default app.** No app may make itself the default:
the installer tells the system which files Letters opens, and the person
picks it. On Windows: Settings > Privacy in Letters has Open Default Apps
Settings…, where Letters can be chosen for each type. On a Mac: select a
file in the Finder, File > Get Info, choose Letters under "Open with", then
Change All.

### What to run on Windows, and what to send back

What could not be checked here because it would change this computer:

1. Run `Letters_0.1.0_x64-setup.exe`. Send: whether it asked for
   administrator rights (it should not), and the folder it installed to
   (`%LOCALAPPDATA%\Letters` expected).
2. In Explorer, look at a .pdf, .docx and .png file. Send a screenshot of
   their icons (Letters' document icons appear only once Letters is the
   default for that type).
3. In Settings > Apps > Default apps, search "Letters". Send a screenshot
   of its page (the list of file types it can open).
4. Double click a PDF and a Word document with Letters set as default, then
   select three PDFs in Explorer and press Enter. Send: how many windows
   opened.
5. Uninstall from Settings > Apps. Send the output of
   `reg query HKCU\Software\Letters /s` and
   `reg query HKCU\Software\RegisteredApplications /v Letters` (both should
   say nothing is found).

### What to run on macOS, and what to send back

Nothing for macOS could be built or run here. On the Mac:

1. `xcode-select --install`, install Rust and Node, then from the
   repository root: `sh tools/fetch-pdfium.sh && sh tools/fetch-tesseract.sh && sh tools/fetch-fonts.sh && npm install`.
2. `node tools/licenses/make-third-party-licenses.mjs`
3. `cd app/shell && npx tauri build 2>&1 | tee ~/letters-build.log`.
   Send `~/letters-build.log` (or its last 100 lines if it is long).
4. `plutil -p target/release/bundle/macos/Letters.app/Contents/Info.plist`.
   Send the output (CFBundleDocumentTypes should list the twelve types).
5. `codesign -dv target/release/bundle/macos/Letters.app 2>&1` and
   `spctl -a -vv target/release/bundle/macos/Letters.app 2>&1`. Send both
   (the app is not signed or notarized yet; this shows what Gatekeeper says).
6. Open the .dmg, drag Letters to Applications, open it (right click >
   Open the first time). Then in the Finder: a PDF > Get Info > Open with >
   Letters > Change All, and double click two PDFs and a .docx. Send: a
   screenshot of the Get Info window, and whether each opened in its own
   window.
7. Letters > Settings… (Command ,): send a screenshot, and one of
   File > Export To… (Command Shift E) with a PDF open.

## Where things are

```
app/shell        the desktop program: windows, file opening, the bridge to the ui
app/shell-core   the parts of the shell that can be tested without a window
app/ui           the interface (Svelte + TypeScript)
core/pdf         opens pdfs and renders pages as tiles (Rust, pdfium)
docs             design notes
tools            scripts: fetching pdfium, the icon source
```

## License

Letters is licensed under either the Apache License 2.0 (`LICENSE-APACHE`)
or the MIT license (`LICENSE-MIT`), at your option. See `LICENSE`.

## Libraries and their licenses

Every library the packaged app is made from is listed, with its license
text, in `THIRD-PARTY-LICENSES.txt`, which `tauri build` writes afresh each
time. The main ones:

| Library | Used for | License |
|---|---|---|
| [PDFium](https://pdfium.googlesource.com/pdfium/) (prebuilt by [pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)) | reading and drawing PDFs | Apache-2.0 |
| [pdfium-render](https://crates.io/crates/pdfium-render) | Rust bindings to PDFium | MIT or Apache-2.0 |
| [Tauri](https://tauri.app) | windows, packaging | MIT or Apache-2.0 |
| [Svelte](https://svelte.dev), [Vite](https://vite.dev) | the interface | MIT |
| [png](https://crates.io/crates/png), [serde](https://serde.rs), [thiserror](https://crates.io/crates/thiserror) | small helpers | MIT or Apache-2.0 |

Nothing here restricts distributing the app. MuPDF, Ghostscript and Poppler
are deliberately not used, because their AGPL/GPL licenses would.
