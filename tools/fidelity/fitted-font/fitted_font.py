"""the fitted document font: ordinary glyphs plus one pre-composed glyph per text segment, chosen by gsub.

a segment (a word, a line, or a run of same-style text) becomes one composite
truetype glyph whose components are the segment's own glyphs, each placed at
its exact source offset in font units. a gsub rule replaces the segment's
typed characters with that glyph, so the docx keeps the real text while word
draws ink placed with font-unit precision instead of its 1/600 in grid.
pure: nothing here talks to word, so it is unit tested (test_fitted_font.py).
"""

from __future__ import annotations

from dataclasses import dataclass, field

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent

# glyf coordinates and composite offsets are signed 16-bit (opentype glyf spec),
# so a pre-composed glyph cannot reach further than this from its origin
MAX_COMPONENT_OFFSET = 32767
# hmtx advance widths are unsigned 16-bit (opentype hmtx spec)
MAX_ADVANCE = 65535
# word's pdf export fails ("unexpected error") on a glyph whose ink reaches
# 32.7 em from its origin, whatever the units per em; 32 em works (c2 --em-width).
# that is 32767 thousandths of an em, the int16 range in pdf glyph space
MAX_INK_REACH_EMS = 32.767
# feature tags we know how to switch on from a docx run (w14 run properties)
STYLISTIC_SET_TAGS = tuple(f"ss{number:02d}" for number in range(1, 21))


@dataclass
class Component:
    glyph: str  # name of an ordinary glyph already in the font
    dx: int  # font units from the composite's origin
    dy: int


@dataclass
class Composite:
    name: str
    chars: list[str]  # the input glyph names the rule consumes, in order
    components: list[Component]
    advance: int  # font units; this is what word lays out, outlines never move layout
    feature: str = "liga"
    # extra glyphs the rule must see before the input (chaining context), for duplicates
    backtrack: list[str] = field(default_factory=list)


def check_offsets(composite: Composite) -> None:
    for component in composite.components:
        if abs(component.dx) > MAX_COMPONENT_OFFSET or abs(component.dy) > MAX_COMPONENT_OFFSET:
            raise ValueError(
                f"{composite.name}: a component sits {component.dx} units away, more than a glyph "
                f"can hold ({MAX_COMPONENT_OFFSET}); split the segment into shorter pieces"
            )


def add_composites(font: TTFont, composites: list[Composite]) -> None:
    """append each composite glyph with its advance; components keep their exact offsets."""
    glyf = font["glyf"]
    hmtx = font["hmtx"]
    order = list(font.getGlyphOrder())
    for composite in composites:
        check_offsets(composite)
        glyph = Glyph()
        glyph.numberOfContours = -1
        # ink-less components (spaces) are left out: they draw nothing, and word's
        # pdf export failed on composites of 30+ chars that held several of them (c3)
        inked = [c for c in composite.components if not _is_empty(glyf[c.glyph])]
        glyph.components = [_component(c) for c in inked]
        if not glyph.components:
            # a segment with no ink (only spaces) still needs a real glyph
            glyph = Glyph()
            glyph.numberOfContours = 0
        glyf.glyphs[composite.name] = glyph
        order.append(composite.name)
        hmtx.metrics[composite.name] = (composite.advance, 0)
    font.setGlyphOrder(order)
    glyf.glyphOrder = order
    for composite in composites:
        glyf[composite.name].recalcBounds(glyf)
        _check_bounds(composite.name, glyf[composite.name])
        check_ink_reach(font, composite.name)
        left = glyf[composite.name].xMin if hasattr(glyf[composite.name], "xMin") else 0
        hmtx.metrics[composite.name] = (composite.advance, left)
    font["maxp"].numGlyphs = len(order)
    # maxComponentElements and friends must cover the new glyphs or validators complain
    font["maxp"].recalc(font)
    font["hhea"].advanceWidthMax = max(width for width, _ in hmtx.metrics.values())


def _check_bounds(name: str, glyph: Glyph) -> None:
    # the glyph's bounding box is stored as int16 too (glyf header), so the
    # far edge of the last component must also fit, not only its offset
    for edge in ("xMin", "yMin", "xMax", "yMax"):
        value = getattr(glyph, edge, 0)
        if abs(value) > MAX_COMPONENT_OFFSET:
            raise ValueError(f"{name}: its {edge} would be {value} units, more than a glyph can hold")


def check_ink_reach(font: TTFont, name: str) -> None:
    glyph = font["glyf"][name]
    reach = max(abs(getattr(glyph, "xMin", 0)), abs(getattr(glyph, "xMax", 0)))
    if reach / font["head"].unitsPerEm >= MAX_INK_REACH_EMS:
        raise ValueError(f"{name}: its ink reaches {reach} units from the origin; word cannot export that")


def _is_empty(glyph: Glyph) -> bool:
    return glyph.numberOfContours == 0


def _component(component: Component) -> GlyphComponent:
    piece = GlyphComponent()
    piece.glyphName = component.glyph
    piece.x = component.dx
    piece.y = component.dy
    # no ROUND_XY_TO_GRID and no USE_MY_METRICS: offsets are exact and the
    # composite keeps its own advance (fontTools adds the size flags itself)
    piece.flags = 0
    return piece


def _fea_name(glyph_name: str) -> str:
    # a backslash makes any glyph name legal in feature syntax, keywords included
    return "\\" + glyph_name


def feature_text(composites: list[Composite]) -> str:
    """opentype feature syntax for every rule: per feature tag, one chaining lookup.

    all rules of a tag sit in one chaining lookup so they are applied in a
    single left-to-right pass: when piece k is reached, piece k-1 has already
    become its own glyph, which is what piece k's backtrack names. that is how
    two occurrences of the same word get different glyphs, and why an edited
    word makes only the pieces after it fall back. rules are tried in order,
    the most specific first: longer context, then longer input."""
    lines = ["languagesystem DFLT dflt;", "languagesystem latn dflt;"]
    by_feature: dict[str, list[Composite]] = {}
    for composite in composites:
        by_feature.setdefault(composite.feature, []).append(composite)
    for tag, rules in by_feature.items():
        groups = _nested_groups(rules)
        for index, group in enumerate(groups):
            lines.append(_substitution_lookup(f"{tag}_sub{index}", group))
        group_of = {rule.name: index for index, group in enumerate(groups) for rule in group}
        ordered = sorted(rules, key=lambda r: (-len(r.backtrack), -len(r.chars)))
        body = [_chain_rule(rule, f"{tag}_sub{group_of[rule.name]}") for rule in ordered]
        lines.append(f"lookup {tag}_chain {{\n" + "\n".join(body) + f"\n}} {tag}_chain;")
        lines.append(f"feature {tag} {{ lookup {tag}_chain; }} {tag};")
    return "\n".join(lines)


def _nested_groups(rules: list[Composite]) -> list[list[Composite]]:
    """substitution lookups the chaining rules call, never two rules with the same first glyph in one.

    a ligature lookup applied at a position takes the longest ligature that
    starts there, so a lookup must not hold a second rule that could start at
    the same glyph; and one lookup holds either single (one input glyph) or
    ligature substitutions, never both (opentype lookup types 1 and 4)."""
    groups: list[list[Composite]] = []
    keys: list[tuple[bool, set[str]]] = []
    for rule in rules:
        is_single = len(rule.chars) == 1
        for group, (group_single, firsts) in zip(groups, keys, strict=True):
            if group_single == is_single and rule.chars[0] not in firsts:
                group.append(rule)
                firsts.add(rule.chars[0])
                break
        else:
            groups.append([rule])
            keys.append((is_single, {rule.chars[0]}))
    return groups


def _substitution_lookup(name: str, rules: list[Composite]) -> str:
    body = [f"  sub {' '.join(_fea_name(g) for g in r.chars)} by {_fea_name(r.name)};" for r in rules]
    return f"lookup {name} {{\n" + "\n".join(body) + f"\n}} {name};"


def _chain_rule(rule: Composite, nested: str) -> str:
    backtrack = " ".join(_fea_name(g) for g in rule.backtrack)
    first, *rest = [_fea_name(g) + "'" for g in rule.chars]
    marked = " ".join([f"{first} lookup {nested}", *rest])
    return f"  sub {backtrack} {marked};" if backtrack else f"  sub {marked};"


def add_rules(font: TTFont, composites: list[Composite]) -> None:
    if "GSUB" in font:
        del font["GSUB"]
    addOpenTypeFeaturesFromString(font, feature_text(composites))


def glyph_name_for_char(font: TTFont, char: str) -> str:
    cmap = font.getBestCmap()
    name = cmap.get(ord(char))
    if name is None:
        raise KeyError(f"the font has no glyph for {char!r} (U+{ord(char):04X})")
    return name


def outline_signature(font: TTFont, glyph_name: str) -> tuple:
    """the glyph's decomposed outline, comparable across fonts that renumber glyphs.

    word subsets the font it writes into its pdf and renames every glyph, so a
    drawn glyph is identified by its points (components resolved), not by id."""
    glyf = font["glyf"]
    coordinates, end_points, _ = glyf[glyph_name].getCoordinates(glyf)
    return (tuple(end_points), tuple((round(x), round(y)) for x, y in coordinates))
