"""corpus documents made by a pdf library, the way slide tools, form tools and
journal pipelines produce them: reportlab with subset-embedded truetype fonts,
plus the standard 14 fonts that are referenced but never embedded.
"""

from __future__ import annotations

import io
import os
import random

from PIL import Image
from reportlab.lib.colors import Color, HexColor
from reportlab.lib.utils import ImageReader
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.pdfgen import canvas

from corpus_builder import fonts
from corpus_builder.text import paragraph_text

SLIDE = (720.0, 405.0)  # 16:9 slides exported to pdf are 10 x 5.625 inches
LETTER = (612.0, 792.0)


def register(role: str, style: str = "regular") -> str:
    name = f"{fonts.family(role).replace(' ', '')}-{style}"
    if name not in pdfmetrics.getRegisteredFontNames():
        pdfmetrics.registerFont(TTFont(name, fonts.font_file(role, style)))
    return name


def wrap(text: str, font: str, size: float, width: float) -> list[str]:
    lines, current = [], ""
    for word in text.split():
        candidate = f"{current} {word}".strip()
        if pdfmetrics.stringWidth(candidate, font, size) > width and current:
            lines.append(current)
            current = word
        else:
            current = candidate
    return lines + ([current] if current else [])


def _photo(width: int, height: int) -> ImageReader:
    image = Image.new("RGB", (width, height))
    rng = random.Random(3)
    pixels = image.load()
    for y in range(height):
        for x in range(width):
            pixels[x, y] = (200 - y // 3 + rng.randint(-8, 8), 150 + x // 5, 90 + rng.randint(-8, 8))
    buffer = io.BytesIO()
    image.save(buffer, "JPEG", quality=85)
    buffer.seek(0)
    return ImageReader(buffer)


def slides(out_dir: str) -> tuple[str, dict]:
    path = os.path.join(out_dir, "slides.pdf")
    title, body = register("sans", "bold"), register("sans")
    pdf = canvas.Canvas(path, pagesize=SLIDE)
    width, height = SLIDE
    decks = [
        ("Waves and Optics", ["Week 5: interference", "Two-slit experiment", "Coherence and path difference"]),
        ("Two-slit interference", ["Bright fringes where the path difference is a whole number of wavelengths",
                                   "Fringe spacing grows with distance to the screen", "Shorter wavelength, closer fringes"]),
        ("Summary", ["Superposition explains interference", "Measure fringe spacing to find the wavelength", "Next week: diffraction"]),
    ]  # fmt: skip
    for index, (heading, bullets) in enumerate(decks):
        pdf.setFillColor(HexColor("#1F3864") if index != 1 else HexColor("#F2F2F2"))
        pdf.rect(0, 0, width, height, stroke=0, fill=1)
        pdf.setFillColor(HexColor("#ED7D31"))
        pdf.rect(0, height - 12, width, 12, stroke=0, fill=1)
        text_color = HexColor("#FFFFFF") if index != 1 else HexColor("#1F3864")
        pdf.setFillColor(text_color)
        pdf.setFont(title, 36)
        pdf.drawString(48, height - 86, heading)
        pdf.setFont(body, 20)
        y = height - 140
        for bullet in bullets:
            for line_index, line in enumerate(wrap(bullet, body, 20, 380 if index == 1 else 600)):
                pdf.drawString(84, y, line)
                if line_index == 0:
                    pdf.circle(66, y + 6, 4, stroke=0, fill=1)  # a curved bullet shape
                y -= 30
            y -= 6
        if index == 1:
            pdf.drawImage(_photo(240, 180), 470, 90, width=200, height=150)
            pdf.setStrokeColor(HexColor("#ED7D31"))
            pdf.setLineWidth(2)
            pdf.roundRect(465, 85, 210, 160, 10, stroke=1, fill=0)
            pdf.setFillColor(HexColor("#404040"))
            pdf.setFont(body, 12)
            pdf.drawString(470, 70, "Fringes on the screen")
        pdf.saveState()
        pdf.translate(width - 20, 40)
        pdf.rotate(90)
        pdf.setFillColor(HexColor("#A5A5A5"))
        pdf.setFont(body, 10)
        pdf.drawString(0, 0, f"PHYS 201 - slide {index + 1}")
        pdf.restoreState()
        pdf.setFillColor(HexColor("#FFC000"))
        pdf.rect(48, 24, 120, 8, stroke=0, fill=1)
        pdf.showPage()
    pdf.save()
    return path, {
        "class": "slides",
        "producer": "reportlab 4 (slide layout), Lato subset-embedded",
        "features": [
            "landscape 720x405",
            "large text",
            "coloured backgrounds",
            "white text",
            "circle bullets (curves)",
            "rounded rectangle",
            "jpeg picture",
            "rotated text",
        ],  # fmt: skip
        "known_phrase": "Two-slit interference",
        "known_sentence": "Fringe spacing grows with distance to the screen",
    }


def form(out_dir: str) -> tuple[str, dict]:
    """a registration form in the standard 14 fonts, which pdfs reference without embedding."""
    path = os.path.join(out_dir, "form.pdf")
    pdf = canvas.Canvas(path, pagesize=LETTER)
    width, height = LETTER
    pdf.setFont("Helvetica-Bold", 16)
    pdf.drawString(54, height - 64, "Course Registration Form")
    pdf.setFont("Times-Roman", 9)
    pdf.drawString(54, height - 80, "Please complete every field in block capitals and return to the faculty office.")
    y = height - 120
    fields = ["Family name", "Given names", "Student number", "Programme", "Email address", "Phone"]
    for label in fields:
        pdf.setFont("Helvetica", 9)
        pdf.drawString(54, y + 4, label)
        pdf.setLineWidth(0.6)
        pdf.rect(170, y - 2, 360, 18, stroke=1, fill=0)
        y -= 30
    pdf.setFont("Helvetica-Bold", 11)
    pdf.drawString(54, y, "Courses this term")
    y -= 18
    pdf.setLineWidth(0.5)
    columns = [54, 130, 380, 470, 558]
    for row in range(7):
        pdf.line(columns[0], y - row * 18, columns[-1], y - row * 18)
    for x in columns:
        pdf.line(x, y, x, y - 6 * 18)
    pdf.setFont("Helvetica-Bold", 8)
    for x, label in zip(columns, ["Code", "Title", "Credits", "Signature"], strict=False):
        pdf.drawString(x + 4, y - 13, label)
    pdf.setFont("Helvetica", 8)
    for row, (code, name) in enumerate(
        [("PHYS201", "Waves and Optics"), ("MATH210", "Linear Algebra"), ("CHEM105", "General Chemistry")], start=1
    ):
        pdf.drawString(columns[0] + 4, y - 13 - row * 18, code)
        pdf.drawString(columns[1] + 4, y - 13 - row * 18, name)
        pdf.drawString(columns[2] + 4, y - 13 - row * 18, "5")
    y -= 6 * 18 + 30
    for index, option in enumerate(["Full time", "Part time", "Exchange student"]):
        pdf.rect(54 + index * 150, y, 9, 9, stroke=1, fill=0)
        pdf.setFont("Helvetica", 9)
        pdf.drawString(68 + index * 150, y + 1, option)
    y -= 40
    pdf.setDash(3, 2)
    pdf.line(54, y, 300, y)
    pdf.setDash()
    pdf.setFont("Times-Italic", 8)
    pdf.drawString(54, y - 10, "Signature of student")
    pdf.setFont("Times-Roman", 6.5)
    small = paragraph_text(60, 4)
    for line_index, line in enumerate(wrap(small, "Times-Roman", 6.5, 500)):
        pdf.drawString(54, 60 - line_index * 8, line)
    # an invisible text layer, as ocr software leaves behind a scanned page
    text_object = pdf.beginText(54, 30)
    text_object.setTextRenderMode(3)
    text_object.setFont("Helvetica", 9)
    text_object.textOut("hidden ocr layer: registration form page one")
    pdf.drawText(text_object)
    pdf.showPage()
    pdf.save()
    return path, {
        "class": "form",
        "producer": "reportlab 4, standard 14 fonts (Helvetica, Times) not embedded",
        "features": [
            "non-embedded standard fonts",
            "boxes and rules",
            "small text 6.5 pt",
            "checkboxes",
            "dashed rule",
            "invisible ocr text",
        ],
        "known_phrase": "Course Registration Form",
        "known_sentence": "Please complete every field in block capitals and return to the faculty office.",
    }


def journal(out_dir: str) -> tuple[str, dict]:
    """a three-column journal page, a different font for each role, odd sizes like real typesetting."""
    path = os.path.join(out_dir, "journal.pdf")
    title, author = register("times", "bold"), register("sans", "italic")
    body, body_italic = register("serif"), register("serif", "italic")
    caption, code = register("arial"), register("mono")
    pdf = canvas.Canvas(path, pagesize=LETTER)
    width, height = LETTER
    pdf.setFillColor(HexColor("#7F1D1D"))
    pdf.setFont(title, 21.5)
    pdf.drawString(50, height - 70, "Low-Cost Sensors in Undergraduate Optics Labs")
    pdf.setFillColor(Color(0.2, 0.2, 0.2))
    pdf.setFont(author, 10.3)
    pdf.drawString(50, height - 88, "C. Writer, D. Reviewer, Department of Physics")
    pdf.setLineWidth(0.8)
    pdf.line(50, height - 98, width - 50, height - 98)
    column_width, gap = (width - 100 - 2 * 14) / 3, 14
    xs = [50 + i * (column_width + gap) for i in range(3)]
    size, leading = 9.3, 11.4
    seed = 70
    for column, x in enumerate(xs):
        y = height - 116
        bottom = 70 if column != 1 else 260
        if column == 0:
            pdf.setFont(body_italic, size)
            for line in wrap(paragraph_text(69, 4), body_italic, size, column_width):
                pdf.drawString(x, y, line)
                y -= leading
            y -= 6
        pdf.setFont(body, size)
        while y > bottom:
            for line in wrap(paragraph_text(seed, 5), body, size, column_width):
                if y <= bottom:
                    break
                pdf.drawString(x, y, line)
                y -= leading
            seed += 1
            y -= 4
        if column == 1:
            # a figure with a caption and a code listing in the middle column
            pdf.drawImage(_photo(200, 120), x, 160, width=column_width, height=90)
            pdf.setFont(caption, 7.9)
            pdf.drawString(x, 150, "Figure 1. Sensor response at three")
            pdf.drawString(x, 141, "wavelengths, normalised.")
            pdf.setFont(code, 7.5)
            for index, line in enumerate(["for t in times:", "    v = read(t)", "    log(t, v)"]):
                pdf.drawString(x + 6, 120 - index * 9, line)
            pdf.setLineWidth(0.4)
            pdf.rect(x, 96, column_width, 34, stroke=1, fill=0)
    # accented names push the body font past one 256-code subset
    pdf.setFont(body, 8)
    accented = "Acknowledgements: Ångström, Dvořák, Łódź, Škoda, Gajšek, Peña, " \
               "Müller, İstanbul, Őrs, Ňovák, Ťichý, Źielinski, Žagar."  # fmt: skip
    pdf.drawString(50, 50, accented)
    extended = "".join(chr(c) for c in range(0x0100, 0x017F) if pdfmetrics.getFont(body).face.charToGlyph.get(c))
    pdf.drawString(50, 38, extended[:64])
    pdf.drawString(50, 28, extended[64:])
    pdf.showPage()
    pdf.save()
    return path, {
        "class": "journal page",
        "producer": "reportlab 4, five fonts subset-embedded (Tinos, Lato, Crimson Text, Arimo, Cousine)",
        "features": [
            "three columns",
            "a font per role",
            "odd sizes 21.5/10.3/9.3/7.9/7.5 pt",
            "coloured title",
            "jpeg figure",
            "code listing in a box",
            "more than 256 glyphs of one font (several subsets)",
        ],  # fmt: skip
        "known_phrase": "Low-Cost Sensors",
        "known_sentence": "C. Writer, D. Reviewer, Department of Physics",
    }


BUILDERS = {"slides": slides, "form": form, "journal": journal}
