// writes THIRD-PARTY-LICENSES.txt: every library the shipped app is made
// from, with its license and its license text, and a list of licenses that
// need a person to look at them.
//
// run from the repository root (needs cargo and node):
//   node tools/licenses/make-third-party-licenses.mjs
//
// rust: `cargo metadata` gives the whole dependency graph; only what the
// desktop program (letters-shell) uses at run time is listed, for windows
// and macos alike (build scripts' helpers are listed too, since they are
// compiled into nothing but are still used to make the program). tests'
// helpers and tools are left out.
// javascript: the ui's package-lock.json; packages marked "dev" are only
// used to build the ui and are not shipped, so they are listed apart.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const out = path.join(root, "THIRD-PARTY-LICENSES.txt");

/** licenses that never restrict shipping the app. anything else is flagged. */
const PERMISSIVE = new Set([
  "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "OFL-1.1", "MPL-2.0",
  "Unicode-3.0", "Unicode-DFS-2016", "0BSD", "CC0-1.0", "BSL-1.0", "Unlicense", "Apache-2.0 WITH LLVM-exception",
  "CDLA-Permissive-2.0", "MIT-0",
]);

/** is every way to take this license expression permissive? "A OR B" needs one, "A AND B" both. */
function acceptable(expression) {
  if (!expression) return false;
  const cleaned = expression.replace(/[()]/g, " ").replace(/\//g, " OR ").trim();
  return cleaned.split(/\s+OR\s+/).some((choice) => choice.split(/\s+AND\s+/).every((part) => PERMISSIVE.has(part.trim())));
}

// notes about parts that aren't crates or npm packages, known from earlier stages
const KNOWN_NOTES = [
  "PDFium (vendor/pdfium, from pdfium-binaries): Apache-2.0 and BSD-3-Clause, shipped as a library beside the program.",
  "Tesseract OCR (vendor/tesseract): Apache-2.0, run as a separate program. Its Windows build brings libiconv-2.dll, which is LGPL-2.1: it is a separate, dynamically loaded library of a separate program and is shipped unchanged with its license, which LGPL allows; flagged so the owner confirms this.",
  "Microsoft Visual C++ runtime: only the MSVC toolchain needs it. This build uses the GNU toolchain (MinGW-w64), whose runtime parts (libgcc, libstdc++, winpthreads) carry the GCC Runtime Library Exception or are statically linked under permissive terms; if the app is ever built with MSVC, check whether vcruntime140.dll may be redistributed (it may, under Microsoft's Visual Studio redistribution terms) before shipping it.",
  "webpki-roots: CDLA-Permissive-2.0 (the list of trusted certificate authorities), permissive.",
  "Bundled fonts (vendor/fonts): SIL Open Font License 1.1 or Apache-2.0; their license files are shipped in fonts/.",
  "pikepdf and axe-core were used only as development tools for checking files and accessibility; they are not part of the app.",
  "Microsoft Office and LibreOffice are used only when the person has installed them, as separate programs; nothing of theirs is shipped.",
];

function cargoPackages() {
  const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--locked"], { cwd: root, maxBuffer: 256 * 1024 * 1024, encoding: "utf8" }));
  const byId = new Map(metadata.packages.map((pkg) => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const workspace = new Set(metadata.workspace_members);
  const start = metadata.packages.find((pkg) => pkg.name === "letters-shell");
  const seen = new Set();
  const queue = [start.id];
  while (queue.length) {
    const id = queue.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id)?.deps ?? []) {
      // normal and build dependencies; dev (test) dependencies are not shipped
      if (dep.dep_kinds.some((kind) => kind.kind === null || kind.kind === "build")) queue.push(dep.pkg);
    }
  }
  return [...seen].filter((id) => !workspace.has(id)).map((id) => byId.get(id)).sort((a, b) => a.name.localeCompare(b.name));
}

function licenseText(pkg) {
  const folder = path.dirname(pkg.manifest_path);
  let names = [];
  try {
    names = fs.readdirSync(folder).filter((name) => /^(licen[cs]e|copying|notice)/i.test(name));
  } catch {
    return "";
  }
  return names.map((name) => `--- ${name} ---\n${fs.readFileSync(path.join(folder, name), "utf8").trim()}`).join("\n\n");
}

function npmPackages() {
  const lockFile = [path.join(root, "app", "ui", "package-lock.json"), path.join(root, "package-lock.json")].find((file) => fs.existsSync(file));
  const lock = JSON.parse(fs.readFileSync(lockFile, "utf8"));
  return Object.entries(lock.packages ?? {})
    .filter(([key]) => key.startsWith("node_modules/") || key.includes("/node_modules/"))
    .map(([key, value]) => ({ name: key.replace(/^.*node_modules\//, ""), version: value.version, license: value.license ?? "", dev: !!value.dev }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

const crates = cargoPackages();
const npm = npmPackages();
const shippedNpm = npm.filter((pkg) => !pkg.dev);
// shipped parts that aren't in either graph but need a person to confirm them
const KNOWN_FLAGS = [
  "libiconv-2.dll (with the OCR engine in tesseract/): LGPL-2.1, a separate library of a separate program, shipped unchanged with its license",
  "Visual C++ runtime: not used by this GNU toolchain build; confirm the redistribution terms if the app is ever built with MSVC",
];

const flagged = [
  ...KNOWN_FLAGS,
  ...crates.filter((pkg) => !acceptable(pkg.license)).map((pkg) => `crate ${pkg.name} ${pkg.version}: ${pkg.license ?? "no license field"}`),
  ...shippedNpm.filter((pkg) => !acceptable(pkg.license)).map((pkg) => `npm ${pkg.name} ${pkg.version}: ${pkg.license || "no license field"}`),
];

const lines = [];
lines.push("Letters: third-party licenses", "", `Made by tools/licenses/make-third-party-licenses.mjs on ${new Date().toISOString().slice(0, 10)}.`, "");
lines.push("== Notes", "", ...KNOWN_NOTES.map((note) => `- ${note}`), "");
lines.push("== For review (not MIT, Apache, BSD, ISC, Zlib, OFL, MPL or similar)", "");
lines.push(...(flagged.length ? flagged.map((item) => `- ${item}`) : ["- none"]), "");
lines.push(`== Rust libraries in the program (${crates.length})`, "");
for (const pkg of crates) lines.push(`${pkg.name} ${pkg.version}  ${pkg.license ?? "(no license field)"}  ${pkg.repository ?? ""}`.trimEnd());
lines.push("", `== JavaScript libraries in the interface (${shippedNpm.length})`, "");
for (const pkg of shippedNpm) lines.push(`${pkg.name} ${pkg.version}  ${pkg.license}`);
lines.push("", `== JavaScript build tools, not shipped (${npm.length - shippedNpm.length})`, "");
for (const pkg of npm.filter((each) => each.dev)) lines.push(`${pkg.name} ${pkg.version}  ${pkg.license}`);
lines.push("", "== License texts of the Rust libraries", "");
for (const pkg of crates) {
  const text = licenseText(pkg);
  lines.push(`### ${pkg.name} ${pkg.version} (${pkg.license ?? "no license field"})`, text || "(no license file in the package; see its repository)", "");
}
fs.writeFileSync(out, lines.join("\n") + "\n");
console.log(`wrote ${out}: ${crates.length} crates, ${shippedNpm.length} shipped npm packages, ${npm.length - shippedNpm.length} build tools`);
console.log(flagged.length ? `for review:\n${flagged.join("\n")}` : "nothing flagged");
