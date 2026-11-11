#!/bin/sh
# downloads the prebuilt pdfium library (apache-2.0) into vendor/pdfium.
# run once after cloning, from the repo root:  sh tools/fetch-pdfium.sh
#
# the version is pinned so everyone builds against the same engine,
# and it matches the pdfium api version the rust bindings were written for.
set -eu

version=8125
case "$(uname -s)" in
  Darwin) package=pdfium-mac-univ; library=lib/libpdfium.dylib ;;
  Linux)  package=pdfium-linux-x64; library=lib/libpdfium.so ;;
  *) echo "use tools/fetch-pdfium.ps1 on windows" >&2; exit 1 ;;
esac

repo_root=$(cd "$(dirname "$0")/.." && pwd)
target="$repo_root/vendor/pdfium"
unpacked=$(mktemp -d)

curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$version/$package.tgz" \
  | tar -xz -C "$unpacked"

# only the library and its license are needed at run time
mkdir -p "$target"
cp "$unpacked/$library" "$target/"
cp "$unpacked/LICENSE" "$target/LICENSE.pdfium.txt"
echo "chromium/$version" > "$target/VERSION"

rm -rf "$unpacked"
echo "pdfium chromium/$version is ready in $target"
