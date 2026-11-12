"""the open-licensed fonts the corpus is written with.

all are sil ofl 1.1 (licenses in fonts\\). arimo only ships as a variable font,
so static regular and bold instances are cut from it with fontTools' instancer;
reportlab and word's pdf export both want static fonts.

word did not reliably pick up embedded bold and italic faces of a family (it
fell back to calibri for headings and captions in the first corpus build), so
for word documents every style is also available as its own family, named
for example "Lato Bold", and used without w:b or w:i.
"""

from __future__ import annotations

import os

from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

from font_prep import rename_family

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT_DIR = os.path.join(HERE, "fonts")
BUILD_DIR = os.path.join(FONT_DIR, "build", "corpus")

# role -> (family, {style: file})
FAMILIES = {
    "serif": ("Crimson Text", {"regular": "CrimsonText-Regular.ttf", "bold": "CrimsonText-Bold.ttf", "italic": "CrimsonText-Italic.ttf"}),
    "sans": ("Lato", {"regular": "Lato-Regular.ttf", "bold": "Lato-Bold.ttf", "italic": "Lato-Italic.ttf"}),
    "times": ("Tinos", {"regular": "Tinos-Regular.ttf", "bold": "Tinos-Bold.ttf"}),
    "mono": ("Cousine", {"regular": "Cousine-Regular.ttf"}),
    "math": ("Noto Sans Math", {"regular": "NotoSansMath-Regular.ttf"}),
    "arial": ("Arimo", {"regular": "build/corpus/Arimo-Regular.ttf", "bold": "build/corpus/Arimo-Bold.ttf"}),
}  # fmt: skip


def _static_arimo(weight: int, path: str) -> None:
    font = TTFont(os.path.join(FONT_DIR, "Arimo-VF.ttf"))
    static = instancer.instantiateVariableFont(font, {"wght": weight}, updateFontNames=True)
    static.save(path)


def ensure_static_fonts() -> None:
    import test_fonts

    test_fonts.ensure_downloads()
    os.makedirs(BUILD_DIR, exist_ok=True)
    for weight, name in ((400, "Arimo-Regular.ttf"), (700, "Arimo-Bold.ttf")):
        path = os.path.join(BUILD_DIR, name)
        if not os.path.exists(path):
            _static_arimo(weight, path)


def font_file(role: str, style: str = "regular") -> str:
    ensure_static_fonts()
    return os.path.join(FONT_DIR, FAMILIES[role][1][style])


def family(role: str) -> str:
    return FAMILIES[role][0]


def style_family_name(role: str, style: str) -> str:
    """the family name a word document uses for one style of a role."""
    base = family(role)
    return base if style == "regular" else f"{base} {style.capitalize()}"


def style_family_file(role: str, style: str) -> str:
    """the font file renamed to its own family, so word treats each style as a regular face."""
    if style == "regular":
        return font_file(role)
    path = os.path.join(BUILD_DIR, f"{family(role).replace(' ', '')}-{style}-family.ttf")
    if not os.path.exists(path):
        font = TTFont(font_file(role, style))
        rename_family(font, style_family_name(role, style))
        # a regular face of its own family: clear bold and italic bits so word does not restyle it
        font["OS/2"].fsSelection = (font["OS/2"].fsSelection & ~0b100001) | 0b1000000
        font["head"].macStyle = 0
        font.save(path)
    return path
