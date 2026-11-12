"""downloading the ofl test fonts and building the renamed variants all experiments use.

the downloaded font files and everything in fonts\\build are not committed,
only the license texts are. run any experiment and it fetches what is missing.
"""

from __future__ import annotations

import os
import urllib.request

import font_prep

HERE = os.path.dirname(os.path.abspath(__file__))
FONT_DIR = os.path.join(HERE, "fonts")
BUILD_DIR = os.path.join(FONT_DIR, "build")

# official upstream sources only, both fonts are under the sil open font license 1.1
DOWNLOADS = {
    "CrimsonText-Regular.ttf": "https://raw.githubusercontent.com/google/fonts/main/ofl/crimsontext/CrimsonText-Regular.ttf",
    "CrimsonText-OFL.txt": "https://raw.githubusercontent.com/google/fonts/main/ofl/crimsontext/OFL.txt",
    "SourceSerif4-Regular.otf": "https://raw.githubusercontent.com/adobe-fonts/source-serif/release/OTF/SourceSerif4-Regular.otf",
    "SourceSerif-LICENSE.md": "https://raw.githubusercontent.com/adobe-fonts/source-serif/release/LICENSE.md",
}
_GOOGLE = "https://raw.githubusercontent.com/google/fonts/main/ofl/"
# round 3 corpus fonts, all sil ofl 1.1 in google/fonts (tinos ships no OFL.txt in its folder;
# fonts\tinos-OFL.txt is the standard ofl text with tinos's copyright line from its METADATA.pb)
CORPUS_DOWNLOADS = {
    "CrimsonText-Bold.ttf": _GOOGLE + "crimsontext/CrimsonText-Bold.ttf",
    "CrimsonText-Italic.ttf": _GOOGLE + "crimsontext/CrimsonText-Italic.ttf",
    "Lato-Regular.ttf": _GOOGLE + "lato/Lato-Regular.ttf",
    "Lato-Bold.ttf": _GOOGLE + "lato/Lato-Bold.ttf",
    "Lato-Italic.ttf": _GOOGLE + "lato/Lato-Italic.ttf",
    "lato-OFL.txt": _GOOGLE + "lato/OFL.txt",
    "Tinos-Regular.ttf": _GOOGLE + "tinos/Tinos-Regular.ttf",
    "Tinos-Bold.ttf": _GOOGLE + "tinos/Tinos-Bold.ttf",
    "Cousine-Regular.ttf": _GOOGLE + "cousine/Cousine-Regular.ttf",
    "cousine-OFL.txt": _GOOGLE + "cousine/OFL.txt",
    "Arimo-VF.ttf": _GOOGLE + "arimo/Arimo%5Bwght%5D.ttf",
    "arimo-OFL.txt": _GOOGLE + "arimo/OFL.txt",
    "NotoSansMath-Regular.ttf": _GOOGLE + "notosansmath/NotoSansMath-Regular.ttf",
    "notosansmath-OFL.txt": _GOOGLE + "notosansmath/OFL.txt",
}

# unique names so word can only get these glyphs from the docx itself.
# source serif has the reserved font name "Source", so the new name must avoid it
TRUETYPE_FAMILY = "Fidelity Probe Serif"
SUBSET_FAMILY = "Fidelity Probe Subset"
CFF_FAMILY = "Fidelity Probe Cff"


def font_path(name: str) -> str:
    return os.path.join(FONT_DIR, name)


def build_path(name: str) -> str:
    os.makedirs(BUILD_DIR, exist_ok=True)
    return os.path.join(BUILD_DIR, name)


def ensure_downloads() -> None:
    os.makedirs(FONT_DIR, exist_ok=True)
    for name, url in {**DOWNLOADS, **CORPUS_DOWNLOADS}.items():
        target = font_path(name)
        if not os.path.exists(target):
            print(f"downloading {url}")
            urllib.request.urlretrieve(url, target)


def assert_not_installed(family: str) -> None:
    if font_prep.is_family_installed(family):
        raise RuntimeError(f"'{family}' is installed on this machine, the embedding test would prove nothing")


def truetype_font() -> str:
    """the renamed full truetype font, rescaled to 1000 units per em, built once.

    pdf /Widths are in 1/1000 em and pdfium keeps them as whole numbers, so with
    crimson's native 1024 units the source pdf's own glyph positions came out
    up to 0.23 pt away from the font's advances in pdfium's reading. at 1000
    units every advance is a whole number and both readings agree exactly."""
    ensure_downloads()
    assert_not_installed(TRUETYPE_FAMILY)
    target = build_path("FidelityProbeSerif-Regular.ttf")
    if not os.path.exists(target):
        font_prep.make_renamed_font(font_path("CrimsonText-Regular.ttf"), target, TRUETYPE_FAMILY, units_per_em=1000)
    return target


def cff_font() -> str:
    ensure_downloads()
    assert_not_installed(CFF_FAMILY)
    target = build_path("FidelityProbeCff-Regular.otf")
    if not os.path.exists(target):
        font_prep.make_renamed_font(font_path("SourceSerif4-Regular.otf"), target, CFF_FAMILY)
    return target
