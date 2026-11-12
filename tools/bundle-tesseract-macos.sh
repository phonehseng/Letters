#!/bin/sh
# makes vendor/tesseract carry its own copy of the tesseract program and
# every library it loads, so the mac app reads scans on a mac without
# homebrew. run after tools/fetch-tesseract.sh, which only links to the
# homebrew install (fine for building and testing, not for shipping):
#   brew install tesseract dylibbundler
#   sh tools/fetch-tesseract.sh && sh tools/bundle-tesseract-macos.sh
#
# dylibbundler copies each library tesseract needs into vendor/tesseract/lib
# and points the program and the libraries at each other through
# @executable_path, so nothing refers to /opt/homebrew or /usr/local any
# more. the check at the end stops the build if anything still does.
#
# the copy is for the processor homebrew built it for (apple silicon on
# github's macos runners). on an intel mac it can't start, and the scan tab
# says text recognition isn't available (core/ocr/src/engine.rs) rather
# than anything failing.
set -eu

repo_root=$(cd "$(dirname "$0")/.." && pwd)
target="$repo_root/vendor/tesseract"
bin="$target/bin/tesseract"

if [ "$(uname -s)" != Darwin ]; then
  echo "this is for macos only" >&2
  exit 1
fi
command -v dylibbundler >/dev/null || { echo "install it with:  brew install dylibbundler" >&2; exit 1; }
[ -e "$bin" ] || { echo "run tools/fetch-tesseract.sh first" >&2; exit 1; }

# the real program behind fetch-tesseract.sh's link (and homebrew's own)
program=$(cd "$(dirname "$bin")" && python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$bin")
rm -f "$bin"
cp "$program" "$bin"
chmod u+w "$bin"

rm -rf "$target/lib"
mkdir -p "$target/lib"
dylibbundler --overwrite-files --bundle-deps --create-dir \
  --fix-file "$bin" \
  --dest-dir "$target/lib" \
  --install-path @executable_path/../lib/

# nothing may still point outside the bundle
if otool -L "$bin" "$target"/lib/*.dylib | grep -E '/opt/homebrew|/usr/local|/opt/local'; then
  echo "the lines above still point outside vendor/tesseract" >&2
  exit 1
fi

# run it with nothing from the build machine's environment to lean on
env -i "$bin" --version | head -n 1
version=$("$bin" --version 2>&1 | head -n 1)
echo "$version (bundled from homebrew for $(uname -m)), tessdata_fast 4.1.0" > "$target/VERSION"
echo "tesseract is bundled in $target"
