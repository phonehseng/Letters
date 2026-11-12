#!/bin/sh
# sets up the tesseract ocr engine (apache-2.0) in vendor/tesseract, with
# english, spanish, french and german language data.
# run once after cloning, from the repo root:  sh tools/fetch-tesseract.sh
#
# windows uses tools/fetch-tesseract.ps1, which downloads a pinned build.
# on macos (and linux) this script does not download the program itself:
# it uses the tesseract already installed by the system's package manager
#   macos:  brew install tesseract        (homebrew)
#   linux:  sudo apt install tesseract-ocr
# and links it into vendor/tesseract/bin. the language data is downloaded
# and checked here on every system, so all of them read text the same way.
#
# not yet run on macos: the steps are the expected ones. a later packaging
# stage should bundle a pinned build the way the windows script does
# (conda-forge publishes osx-64 and osx-arm64 builds of the same version).
set -eu

tessdata_url=https://github.com/tesseract-ocr/tessdata_fast/raw/4.1.0
repo_root=$(cd "$(dirname "$0")/.." && pwd)
target="$repo_root/vendor/tesseract"

program=$(command -v tesseract || true)
if [ -z "$program" ]; then
  echo "tesseract is not installed." >&2
  case "$(uname -s)" in
    Darwin) echo "install it with:  brew install tesseract   then run this script again" >&2 ;;
    *) echo "install it with your package manager (for example: sudo apt install tesseract-ocr), then run this script again" >&2 ;;
  esac
  exit 1
fi
version=$("$program" --version 2>&1 | head -n 1)
case "$version" in
  "tesseract 5"*) ;;
  *) echo "Letters needs tesseract 5; found: $version" >&2; exit 1 ;;
esac

# the same files and checksums as tools/fetch-tesseract.ps1
checksum() {
  case "$1" in
    eng) echo 7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2 ;;
    spa) echo 6f2e04d02774a18f01bed44b1111f2cd7f3ba7ac9dc4373cd3f898a40ea6b464 ;;
    fra) echo ced037562e8c80c13122dece28dd477d399af80911a28791a66a63ac1e3445ca ;;
    deu) echo 19d219bbb6672c869d20a9636c6816a81eb9a71796cb93ebe0cb1530e2cdb22d ;;
    osd) echo 9cf5d576fcc47564f11265841e5ca839001e7e6f38ff7f7aacf46d15a96b00ff ;;
  esac
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}

staging=$(mktemp -d)
mkdir -p "$staging/bin" "$staging/tessdata" "$staging/licenses"
for language in eng spa fra deu osd; do
  echo "fetching language data $language"
  file="$staging/tessdata/$language.traineddata"
  curl -fsSL "$tessdata_url/$language.traineddata" -o "$file"
  actual=$(sha256 "$file")
  if [ "$actual" != "$(checksum "$language")" ]; then
    echo "$language.traineddata did not match its checksum. nothing was installed." >&2
    rm -rf "$staging"
    exit 1
  fi
done
curl -fsSL "$tessdata_url/LICENSE" -o "$staging/licenses/tessdata_fast-LICENSE.txt"
ln -s "$program" "$staging/bin/tesseract"
echo "$version (system install at $program), tessdata_fast 4.1.0" > "$staging/VERSION"

rm -rf "$target"
mkdir -p "$(dirname "$target")"
mv "$staging" "$target"
echo "tesseract is ready in $target"
