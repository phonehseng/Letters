#!/bin/sh
# downloads the prebuilt pdfium library (apache-2.0) into vendor/pdfium.
# run once after cloning, from the repo root:  sh tools/fetch-pdfium.sh
#
# the version is pinned so everyone builds against the same engine,
# and it matches the pdfium api version the rust bindings were written for.
set -eu

version=8125

# each archive's sha-256 was recorded when the version was pinned. a
# download that does not match is thrown away, so a tampered or damaged
# file can never end up inside the app.
case "$(uname -s)-$(uname -m)" in
  Darwin-*)
    package=pdfium-mac-univ; library=lib/libpdfium.dylib
    sha256=0cf06803b9a4acc489fc967a1b05990622ca1cf41fa027ff9cc89eeab9b19279 ;;
  Linux-x86_64)
    package=pdfium-linux-x64; library=lib/libpdfium.so
    sha256=e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659 ;;
  Linux-aarch64|Linux-arm64)
    package=pdfium-linux-arm64; library=lib/libpdfium.so
    sha256=8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa ;;
  *) echo "no pdfium package for this system. on windows use tools/fetch-pdfium.ps1" >&2; exit 1 ;;
esac

repo_root=$(cd "$(dirname "$0")/.." && pwd)
target="$repo_root/vendor/pdfium"
work=$(mktemp -d)
archive="$work/$package.tgz"

curl -fsSL -o "$archive" \
  "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$version/$package.tgz"

# macos ships shasum, most linux systems ship sha256sum
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$archive" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$archive" | cut -d' ' -f1)
fi
if [ "$actual" != "$sha256" ]; then
  rm -rf "$work"
  echo "the pdfium download does not match its recorded checksum (got $actual)." >&2
  echo "nothing was installed. try again, and if it keeps failing do not use the file." >&2
  exit 1
fi

mkdir -p "$work/unpacked" "$target"
tar -xzf "$archive" -C "$work/unpacked"

# only the library and its license are needed at run time
cp "$work/unpacked/$library" "$target/"
cp "$work/unpacked/LICENSE" "$target/LICENSE.pdfium.txt"
echo "chromium/$version" > "$target/VERSION"

rm -rf "$work"
echo "pdfium chromium/$version is ready in $target"
