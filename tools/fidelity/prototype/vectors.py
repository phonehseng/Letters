"""vector content: simple paths become drawingml shapes, the rest becomes a picture.

simple means what word can draw exactly: axis-aligned rectangles (filled or
stroked) and straight stroked segments, solid colour, no dash, no transparency,
no clip that cuts them. everything else (curves, dashes, shadings, patterns,
transparency, real clipping) is rasterised, and only that vector content: the
picture is rendered from a page that holds nothing but those items, so no text
ever ends up as pixels.
"""

from __future__ import annotations

import io
from dataclasses import dataclass

import pikepdf
import pypdfium2 as pdfium

from prototype.pdf_content import PageContent, PathItem

RASTER_DPI = 300
AXIS_TOLERANCE = 0.01


@dataclass
class RectShape:
    left: float
    bottom: float
    right: float
    top: float
    fill: tuple[float, float, float] | None
    stroke: tuple[float, float, float] | None
    line_width: float
    sequence: int


@dataclass
class LineShape:
    x0: float
    y0: float
    x1: float
    y1: float
    color: tuple[float, float, float]
    line_width: float
    sequence: int


@dataclass
class RasterPicture:
    left: float
    bottom: float
    right: float
    top: float
    png: bytes
    items: int
    sequence: int


def _subpaths(segments):
    current = []
    for op, points in segments:
        if op in ("m", "re") and current:
            yield current
            current = []
        current.append((op, points))
    if current:
        yield current


def _as_rect(subpath) -> tuple[float, float, float, float] | None:
    if len(subpath) == 1 and subpath[0][0] == "re":
        points = subpath[0][1]
    else:
        points = [p for op, pts in subpath if op in ("m", "l") for p in pts]
        if (
            len(points) == 5
            and abs(points[0][0] - points[4][0]) < AXIS_TOLERANCE
            and abs(points[0][1] - points[4][1]) < AXIS_TOLERANCE
        ):
            points = points[:4]
        if len(points) != 4 or any(op not in ("m", "l", "h") for op, _ in subpath):
            return None
    xs, ys = sorted({round(p[0], 3) for p in points}), sorted({round(p[1], 3) for p in points})
    if len(xs) > 2 or len(ys) > 2:
        return None  # rotated or skewed rectangle
    return min(xs), min(ys), max(xs), max(ys)


def _clip_is_harmless(item: PathItem) -> bool:
    left, bottom, right, top = item.bbox()
    for clip in item.clip:
        rects = [_as_rect(sub) for sub in _subpaths(clip)]
        if len(rects) != 1 or rects[0] is None:
            return False
        cl, cb, cr, ct = rects[0]
        if left < cl - 0.5 or bottom < cb - 0.5 or right > cr + 0.5 or top > ct + 0.5:
            return False
    return True


def classify(item: PathItem) -> list | None:
    """drawingml shapes for a simple path, or None if it must be rasterised."""
    if item.has_curves or item.dashed or item.alpha < 1 or item.pattern or not _clip_is_harmless(item):
        return None
    shapes = []
    for subpath in _subpaths(item.segments):
        rect = _as_rect(subpath) if (item.fill or any(op in ("h", "re") for op, _ in subpath)) else None
        if rect is not None:
            shapes.append(RectShape(*rect, item.fill_color, item.stroke_color, item.line_width, item.sequence))
            continue
        if item.fill:
            return None  # a filled polygon: keep it exact as pixels rather than guess
        points = [p for op, pts in subpath if op in ("m", "l") for p in pts]
        if any(op not in ("m", "l") for op, _ in subpath) or len(points) < 2:
            return None
        for a, b in zip(points, points[1:], strict=False):
            shapes.append(LineShape(a[0], a[1], b[0], b[1], item.stroke_color, item.line_width, item.sequence))
    return shapes


def rasterise(pdf_path: str, page: PageContent, items: list[PathItem]) -> RasterPicture | None:
    """one transparent picture holding only the given vector items of a page."""
    if not items:
        return None
    left = min(i.bbox()[0] for i in items)
    bottom = min(i.bbox()[1] for i in items)
    right = max(i.bbox()[2] for i in items)
    top = max(i.bbox()[3] for i in items)
    left, bottom = max(0.0, left - 1), max(0.0, bottom - 1)
    right, top = min(page.width, right + 1), min(page.height, top + 1)
    with pikepdf.open(pdf_path) as source:
        resources = source.pages[page.index].obj.get("/Resources")
        only_vectors = pikepdf.new()
        only_vectors.add_blank_page(page_size=(page.width, page.height))
        target = only_vectors.pages[0]
        if resources is not None:
            # copy_foreign needs an indirect object, page resources are often direct
            if not resources.is_indirect:
                resources = source.make_indirect(resources)
            target.obj.Resources = only_vectors.copy_foreign(resources)
        target.obj.Contents = only_vectors.make_stream(b"".join(i.raw for i in items))
        buffer = io.BytesIO()
        only_vectors.save(buffer)
    document = pdfium.PdfDocument(buffer.getvalue())
    scale = RASTER_DPI / 72
    bitmap = document[0].render(scale=scale, fill_color=(255, 255, 255, 0), may_draw_forms=False)
    image = bitmap.to_pil()
    crop = (
        int(left * scale),
        int((page.height - top) * scale),
        int(right * scale) + 1,
        int((page.height - bottom) * scale) + 1,
    )
    image = image.crop(crop)
    out = io.BytesIO()
    image.save(out, "PNG")
    document.close()
    first = min(i.sequence for i in items)
    return RasterPicture(crop[0] / scale, page.height - crop[3] / scale, crop[2] / scale, page.height - crop[1] / scale,
                         out.getvalue(), len(items), first)  # fmt: skip
