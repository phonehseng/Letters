"""a pdf content stream interpreter that records glyphs, paths and images.

pdfium's text page gives unicode and pen origins, but not the char codes or
the font program behind each glyph, so the text operators are interpreted
here (pdf 1.7, 9.4): every glyph gets its true pen origin from the text
rendering matrix, its code, its font and its advance. paths and images come
from the same pass, so their order relative to text is the page's own.
coordinates are pdf user space of the page (points, origin bottom left of the
media box).
"""

from __future__ import annotations

import base64
import io
import math
from dataclasses import dataclass, field

import pikepdf

from prototype.pdf_fonts import PdfFont, read_font

Matrix = tuple[float, float, float, float, float, float]
IDENTITY: Matrix = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def multiply(m: Matrix, n: Matrix) -> Matrix:
    """m then n, in pdf's row-vector convention (m x n)."""
    a, b, c, d, e, f = m
    p, q, r, s, t, u = n
    return (a * p + b * r, a * q + b * s, c * p + d * r, c * q + d * s, e * p + f * r + t, e * q + f * s + u)


def apply(m: Matrix, x: float, y: float) -> tuple[float, float]:
    a, b, c, d, e, f = m
    return a * x + c * y + e, b * x + d * y + f


def scale_of(m: Matrix) -> float:
    """uniform scale factor of a matrix, used for line widths."""
    return math.sqrt(abs(m[0] * m[3] - m[1] * m[2]))


@dataclass
class Glyph:
    page: int
    font: PdfFont
    code: int
    text: str
    x: float  # pen origin
    y: float
    size: float  # font size in page space, along the glyph's vertical axis
    horizontal_ratio: float  # horizontal / vertical scale, 1 unless Tz or a stretched matrix
    angle: float  # degrees, counter-clockwise
    advance: float  # pen advance in page space
    color: tuple[float, float, float]
    render_mode: int
    sequence: int  # order of appearance on the page


@dataclass
class PathItem:
    page: int
    segments: list[tuple[str, list[tuple[float, float]]]]  # page space: m, l, c, h, re
    fill: bool
    stroke: bool
    even_odd: bool
    fill_color: tuple[float, float, float] | None
    stroke_color: tuple[float, float, float] | None
    line_width: float
    dashed: bool
    alpha: float
    clip: list[list[tuple[str, list[tuple[float, float]]]]]
    pattern: bool
    raw: bytes  # operators that redraw exactly this item, for rasterising
    sequence: int

    @property
    def has_curves(self) -> bool:
        return any(op == "c" for op, _ in self.segments)

    def bbox(self) -> tuple[float, float, float, float]:
        points = [p for _, pts in self.segments for p in pts]
        xs, ys = [p[0] for p in points], [p[1] for p in points]
        half = self.line_width / 2 if self.stroke else 0
        return min(xs) - half, min(ys) - half, max(xs) + half, max(ys) + half


@dataclass
class ImageItem:
    page: int
    matrix: Matrix  # unit square to page space
    data: bytes
    extension: str  # jpeg or png
    pixel_size: tuple[int, int]
    sequence: int


@dataclass
class PageContent:
    index: int
    width: float
    height: float
    rotate: int
    glyphs: list[Glyph] = field(default_factory=list)
    paths: list[PathItem] = field(default_factory=list)
    images: list[ImageItem] = field(default_factory=list)
    shadings: list[PathItem] = field(default_factory=list)
    problems: list[str] = field(default_factory=list)


@dataclass
class GraphicsState:
    ctm: Matrix = IDENTITY
    fill_color: tuple[float, float, float] = (0.0, 0.0, 0.0)
    stroke_color: tuple[float, float, float] = (0.0, 0.0, 0.0)
    fill_space: str = "DeviceGray"
    stroke_space: str = "DeviceGray"
    fill_pattern: bool = False
    stroke_pattern: bool = False
    line_width: float = 1.0
    dashed: bool = False
    fill_alpha: float = 1.0
    stroke_alpha: float = 1.0
    clip: list = field(default_factory=list)
    # text state survives BT/ET and is saved with q
    char_spacing: float = 0.0
    word_spacing: float = 0.0
    horizontal_scaling: float = 1.0
    leading: float = 0.0
    font: PdfFont | None = None
    font_size: float = 0.0
    render_mode: int = 0
    rise: float = 0.0
    raw_state: list = field(default_factory=list)  # colour and gs operators, replayed when rasterising

    def copy(self) -> GraphicsState:
        clone = GraphicsState(**self.__dict__)
        clone.clip = list(self.clip)
        clone.raw_state = list(self.raw_state)
        return clone


def _to_rgb(space: str, values: list[float]) -> tuple[float, float, float]:
    if len(values) == 1:
        return (values[0],) * 3
    if len(values) == 3:
        return tuple(values)  # type: ignore[return-value]
    if len(values) == 4:
        c, m, y, k = values
        return ((1 - c) * (1 - k), (1 - m) * (1 - k), (1 - y) * (1 - k))
    return (0.0, 0.0, 0.0)


def _color_space_components(resources, name: str) -> str:
    if name in ("DeviceGray", "DeviceRGB", "DeviceCMYK", "Pattern"):
        return name
    spaces = resources.get("/ColorSpace") if resources is not None else None
    entry = spaces.get("/" + name) if spaces is not None else None
    if isinstance(entry, pikepdf.Array) and len(entry) and str(entry[0]) == "/ICCBased":
        components = int(entry[1].get("/N", 3))
        return {1: "DeviceGray", 3: "DeviceRGB", 4: "DeviceCMYK"}.get(components, "DeviceRGB")
    if isinstance(entry, pikepdf.Array) and len(entry) and str(entry[0]) == "/Pattern":
        return "Pattern"
    return "DeviceRGB"


def _ops_bytes(operands, operator: str) -> bytes:
    return pikepdf.unparse_content_stream([(operands, pikepdf.Operator(operator))])


class Interpreter:
    def __init__(self, page_index: int, content: PageContent, origin: tuple[float, float]):
        self.page_index = page_index
        self.content = content
        self.font_cache: dict = {}
        self.sequence = 0
        self.origin_shift: Matrix = (1, 0, 0, 1, -origin[0], -origin[1])

    def next_sequence(self) -> int:
        self.sequence += 1
        return self.sequence

    def font_for(self, font_dict) -> PdfFont:
        key = font_dict.objgen if font_dict.is_indirect else id(font_dict)
        if key not in self.font_cache:
            self.font_cache[key] = read_font(font_dict)
        return self.font_cache[key]

    # ------------------------------------------------------------ the operator loop

    def run(self, stream_owner, resources, state: GraphicsState, depth: int = 0) -> None:
        if depth > 8:
            self.content.problems.append("form xobjects nested too deep, rest skipped")
            return
        stack = []
        text_matrix = line_matrix = IDENTITY
        path: list = []
        path_raw: list = []
        pending_clip = None
        for operands, operator in pikepdf.parse_content_stream(stream_owner):
            op = str(operator)
            values = [_number(v) for v in operands]
            if op == "q":
                stack.append(state.copy())
            elif op == "Q":
                if stack:
                    state = stack.pop()
            elif op == "cm":
                state.ctm = multiply(tuple(values), state.ctm)  # type: ignore[arg-type]
            elif op in ("g", "rg", "k"):
                state.fill_color = _to_rgb("", values)
                state.fill_pattern = False
                state.raw_state.append(_ops_bytes(operands, op))
            elif op in ("G", "RG", "K"):
                state.stroke_color = _to_rgb("", values)
                state.stroke_pattern = False
                state.raw_state.append(_ops_bytes(operands, op))
            elif op == "cs":
                state.fill_space = _color_space_components(resources, str(operands[0])[1:])
                state.raw_state.append(_ops_bytes(operands, op))
            elif op == "CS":
                state.stroke_space = _color_space_components(resources, str(operands[0])[1:])
                state.raw_state.append(_ops_bytes(operands, op))
            elif op in ("sc", "scn"):
                state.raw_state.append(_ops_bytes(operands, op))
                if state.fill_space == "Pattern":
                    state.fill_pattern = True
                else:
                    state.fill_color = _to_rgb(state.fill_space, [v for v in values if isinstance(v, float)])
            elif op in ("SC", "SCN"):
                state.raw_state.append(_ops_bytes(operands, op))
                if state.stroke_space == "Pattern":
                    state.stroke_pattern = True
                else:
                    state.stroke_color = _to_rgb(state.stroke_space, [v for v in values if isinstance(v, float)])
            elif op == "w":
                state.line_width = values[0]
                state.raw_state.append(_ops_bytes(operands, op))
            elif op in ("J", "j", "M"):
                state.raw_state.append(_ops_bytes(operands, op))
            elif op == "d":
                state.dashed = len(operands[0]) > 0
                state.raw_state.append(_ops_bytes(operands, op))
            elif op == "gs":
                self._ext_g_state(resources, str(operands[0]), state)
                state.raw_state.append(_ops_bytes(operands, op))
            # ---------------------------------------------------- paths
            elif op in ("m", "l", "c", "v", "y", "h", "re"):
                path.append((op, values))
                path_raw.append(_ops_bytes(operands, op))
            elif op in ("W", "W*"):
                pending_clip = op
            elif op in ("S", "s", "f", "F", "f*", "B", "B*", "b", "b*", "n"):
                if path:
                    segments = self._page_segments(path, state.ctm)
                    if op != "n":
                        self._record_path(segments, op, state, resources, path_raw)
                    if pending_clip:
                        clip_raw = b"".join(path_raw) + (b" W n\n" if pending_clip == "W" else b" W* n\n")
                        state.clip.append((segments, state.ctm, clip_raw))
                path, path_raw, pending_clip = [], [], None
            elif op == "sh":
                self._record_shading(operands, state, resources)
            # ---------------------------------------------------- text
            elif op == "BT":
                text_matrix = line_matrix = IDENTITY
            elif op == "Tf":
                font_dict = resources.Font[operands[0]] if resources is not None and "/Font" in resources else None
                state.font = self.font_for(font_dict) if font_dict is not None else None
                state.font_size = values[1]
            elif op == "Tc":
                state.char_spacing = values[0]
            elif op == "Tw":
                state.word_spacing = values[0]
            elif op == "Tz":
                state.horizontal_scaling = values[0] / 100
            elif op == "TL":
                state.leading = values[0]
            elif op == "Tr":
                state.render_mode = int(values[0])
            elif op == "Ts":
                state.rise = values[0]
            elif op in ("Td", "TD"):
                if op == "TD":
                    state.leading = -values[1]
                line_matrix = multiply((1, 0, 0, 1, values[0], values[1]), line_matrix)
                text_matrix = line_matrix
            elif op == "Tm":
                line_matrix = text_matrix = tuple(values)  # type: ignore[assignment]
            elif op == "T*":
                line_matrix = multiply((1, 0, 0, 1, 0, -state.leading), line_matrix)
                text_matrix = line_matrix
            elif op in ("Tj", "'", '"'):
                if op in ("'", '"'):
                    if op == '"':
                        state.word_spacing, state.char_spacing = values[0], values[1]
                    line_matrix = multiply((1, 0, 0, 1, 0, -state.leading), line_matrix)
                    text_matrix = line_matrix
                text_matrix = self._show(bytes(operands[-1]), text_matrix, state)
            elif op == "TJ":
                for item in operands[0]:
                    if isinstance(item, pikepdf.String):
                        text_matrix = self._show(bytes(item), text_matrix, state)
                    else:
                        shift = -float(item) / 1000 * state.font_size * state.horizontal_scaling
                        text_matrix = multiply((1, 0, 0, 1, shift, 0), text_matrix)
            # ---------------------------------------------------- xobjects
            elif op == "Do":
                self._do(operands[0], resources, state, depth)
            elif op == "INLINE IMAGE":
                self._record_image(operands[0], state)

    # ------------------------------------------------------------ text

    def _show(self, data: bytes, text_matrix: Matrix, state: GraphicsState) -> Matrix:
        font = state.font
        if font is None:
            self.content.problems.append("text shown without a font")
            return text_matrix
        size, scaling = state.font_size, state.horizontal_scaling
        for code in font.split_codes(data):
            rendering = multiply((size * scaling, 0, 0, size, 0, state.rise), multiply(text_matrix, state.ctm))
            rendering = multiply(rendering, self.origin_shift)
            vertical = math.hypot(rendering[2], rendering[3])
            horizontal = math.hypot(rendering[0], rendering[1])
            advance_text = font.width(code) / 1000 * size + state.char_spacing
            if code == 32 and font.code_bytes == 1:
                advance_text += state.word_spacing
            advance_text *= scaling
            page_matrix = multiply(text_matrix, state.ctm)
            self.content.glyphs.append(
                Glyph(
                    page=self.page_index,
                    font=font,
                    code=code,
                    text=font.text(code),
                    x=rendering[4],
                    y=rendering[5],
                    size=vertical,
                    horizontal_ratio=horizontal / vertical if vertical else 1.0,
                    angle=math.degrees(math.atan2(rendering[1], rendering[0])),
                    advance=advance_text * math.hypot(page_matrix[0], page_matrix[1]),
                    color=state.fill_color,
                    render_mode=state.render_mode,
                    sequence=self.next_sequence(),
                )
            )
            text_matrix = multiply((1, 0, 0, 1, advance_text, 0), text_matrix)
        return text_matrix

    # ------------------------------------------------------------ paths

    def _page_segments(self, path, ctm: Matrix):
        to_page = multiply(ctm, self.origin_shift)
        segments = []
        current = (0.0, 0.0)
        start = (0.0, 0.0)
        for op, values in path:
            if op == "re":
                x, y, w, h = values
                corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                segments.append(("re", [apply(to_page, *p) for p in corners]))
                current = start = (x, y)
            elif op == "m":
                current = start = (values[0], values[1])
                segments.append(("m", [apply(to_page, *current)]))
            elif op == "l":
                current = (values[0], values[1])
                segments.append(("l", [apply(to_page, *current)]))
            elif op in ("c", "v", "y"):
                if op == "c":
                    points = [(values[0], values[1]), (values[2], values[3]), (values[4], values[5])]
                elif op == "v":
                    points = [current, (values[0], values[1]), (values[2], values[3])]
                else:
                    points = [(values[0], values[1]), (values[2], values[3]), (values[2], values[3])]
                current = points[-1]
                segments.append(("c", [apply(to_page, *p) for p in points]))
            elif op == "h":
                current = start
                segments.append(("h", []))
        return segments

    def _record_path(self, segments, op: str, state: GraphicsState, resources, path_raw) -> None:
        fill = op in ("f", "F", "f*", "B", "B*", "b", "b*")
        stroke = op in ("S", "s", "B", "B*", "b", "b*")
        if op in ("s", "b", "b*"):
            segments = segments + [("h", [])]
        raw = self._raw_item(state, b"".join(path_raw) + f" {op}\n".encode())
        self.content.paths.append(
            PathItem(
                page=self.page_index,
                segments=segments,
                fill=fill,
                stroke=stroke,
                even_odd=op in ("f*", "B*", "b*"),
                fill_color=state.fill_color if fill else None,
                stroke_color=state.stroke_color if stroke else None,
                line_width=state.line_width * scale_of(state.ctm),
                dashed=state.dashed,
                alpha=min(state.fill_alpha if fill else 1.0, state.stroke_alpha if stroke else 1.0),
                clip=[segs for segs, _, _ in state.clip],
                pattern=(fill and state.fill_pattern) or (stroke and state.stroke_pattern),
                raw=raw,
                sequence=self.next_sequence(),
            )
        )

    def _raw_item(self, state: GraphicsState, body: bytes) -> bytes:
        """operators that redraw one item on its own: clips, colours, then the item."""
        # clips are replayed in page space, so each keeps working after the item's cm
        parts = [b"q\n"] + [_clip_in_page_space(segs) for segs, _, _ in state.clip]
        parts.append(_matrix_bytes(multiply(state.ctm, self.origin_shift)) + b" cm\n")
        parts.extend(state.raw_state)
        parts.append(body)
        parts.append(b"Q\n")
        return b"".join(parts)

    def _record_shading(self, operands, state: GraphicsState, resources) -> None:
        raw = self._raw_item(state, _ops_bytes(operands, "sh"))
        width, height = self.content.width, self.content.height
        box = [("re", [(0, 0), (width, 0), (width, height), (0, height)])]
        segments = state.clip[-1][0] if state.clip else box
        self.content.shadings.append(
            PathItem(self.page_index, segments, True, False, False, None, None, 0.0, False, 1.0, [], True, raw,
                     self.next_sequence())
        )  # fmt: skip

    def _ext_g_state(self, resources, name: str, state: GraphicsState) -> None:
        states = resources.get("/ExtGState") if resources is not None else None
        entry = states.get(name) if states is not None else None
        if entry is None:
            return
        if "/ca" in entry:
            state.fill_alpha = float(entry.ca)
        if "/CA" in entry:
            state.stroke_alpha = float(entry.CA)
        if "/LW" in entry:
            state.line_width = float(entry.LW)

    # ------------------------------------------------------------ xobjects and images

    def _do(self, name, resources, state: GraphicsState, depth: int) -> None:
        xobjects = resources.get("/XObject") if resources is not None else None
        xobject = xobjects.get(name) if xobjects is not None else None
        if xobject is None:
            return
        subtype = str(xobject.get("/Subtype", ""))
        if subtype == "/Image":
            self._record_image(xobject, state)
        elif subtype == "/Form":
            inner = state.copy()
            matrix = tuple(float(v) for v in xobject.get("/Matrix", [1, 0, 0, 1, 0, 0]))
            inner.ctm = multiply(matrix, state.ctm)  # type: ignore[arg-type]
            self.run(xobject, xobject.get("/Resources", resources), inner, depth + 1)

    def _record_image(self, image_object, state: GraphicsState) -> None:
        try:
            if isinstance(image_object, pikepdf.PdfInlineImage):
                picture = image_object
                pil = picture.as_pil_image()
                data, extension = _png_bytes(pil), "png"
            else:
                picture = pikepdf.PdfImage(image_object)
                jpeg = _original_jpeg(image_object)
                has_mask = "/SMask" in image_object or "/Mask" in image_object
                if (
                    jpeg is not None
                    and not has_mask
                    and str(image_object.get("/ColorSpace", "")) in ("/DeviceRGB", "/DeviceGray")
                ):
                    # the original jpeg bytes, untouched, so the picture is not re-encoded
                    data, extension = jpeg, "jpeg"
                else:
                    pil = picture.as_pil_image()
                    if "/SMask" in image_object:
                        mask = pikepdf.PdfImage(image_object.SMask).as_pil_image().convert("L").resize(pil.size)
                        pil = pil.convert("RGBA")
                        pil.putalpha(mask)
                    data, extension = _png_bytes(pil), "png"
            size = (int(picture.width), int(picture.height))
        except Exception as error:  # an image we cannot decode is reported, not guessed
            self.content.problems.append(f"image skipped: {error}")
            return
        matrix = multiply(state.ctm, self.origin_shift)
        self.content.images.append(ImageItem(self.page_index, matrix, data, extension, size, self.next_sequence()))


def _original_jpeg(image_object) -> bytes | None:
    """the jpeg file inside an image stream: plain DCTDecode, or wrapped in ASCII85 the way reportlab writes it."""
    filters = image_object.get("/Filter")
    names = [str(f) for f in filters] if isinstance(filters, pikepdf.Array) else [str(filters)]
    if names == ["/DCTDecode"]:
        return image_object.read_raw_bytes()
    if names == ["/ASCII85Decode", "/DCTDecode"]:
        raw = image_object.read_raw_bytes().strip()
        if raw.endswith(b"~>"):
            raw = raw[:-2]
        return base64.a85decode(raw.replace(b"\n", b"").replace(b"\r", b""), ignorechars=b" \t")
    return None


def _number(value):
    """operands as floats where they are numbers, everything else unchanged."""
    if isinstance(value, (pikepdf.Name, pikepdf.String, pikepdf.Array, pikepdf.Dictionary, pikepdf.Operator)):
        return value
    try:
        return float(value)
    except (TypeError, ValueError):
        return value


def _matrix_bytes(m: Matrix) -> bytes:
    return " ".join(f"{v:.6f}" for v in m).encode()


def _clip_in_page_space(segments) -> bytes:
    return _segments_bytes(segments) + b" W n\n"


def _segments_bytes(segments) -> bytes:
    parts = []
    for op, points in segments:
        if op == "re":
            (x0, y0), (x1, _), _, (_, y3) = points
            parts.append(f"{x0:.4f} {y0:.4f} m {x1:.4f} {y0:.4f} l {x1:.4f} {y3:.4f} l {x0:.4f} {y3:.4f} l h")
        elif op in ("m", "l"):
            parts.append(f"{points[0][0]:.4f} {points[0][1]:.4f} {op}")
        elif op == "c":
            parts.append(" ".join(f"{x:.4f} {y:.4f}" for x, y in points) + " c")
        elif op == "h":
            parts.append("h")
    return " ".join(parts).encode()


def _png_bytes(pil) -> bytes:
    buffer = io.BytesIO()
    if pil.mode not in ("RGB", "RGBA", "L", "LA", "P"):
        pil = pil.convert("RGB")
    pil.save(buffer, "PNG")
    return buffer.getvalue()


def read_document(pdf_path: str) -> list[PageContent]:
    pages = []
    with pikepdf.open(pdf_path) as pdf:
        for index, page in enumerate(pdf.pages):
            box = [float(v) for v in page.mediabox]
            content = PageContent(index, box[2] - box[0], box[3] - box[1], int(page.obj.get("/Rotate", 0)))
            if content.rotate:
                content.problems.append(f"page rotated by {content.rotate} degrees, coordinates not rotated")
            interpreter = Interpreter(index, content, (box[0], box[1]))
            interpreter.run(page, page.obj.get("/Resources"), GraphicsState())
            pages.append(content)
    return pages
