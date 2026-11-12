"""corpus documents made the way students make them: written in word, exported by word.

each builder returns (docx path, manifest entry). make_corpus.py exports the
docx to pdf with word's own ExportAsFixedFormat, which gives real word pdfs:
subset-embedded truetype fonts, justified lines with word spacing, tables
drawn as filled rectangles, footnote separators, pictures and shapes.
"""

from __future__ import annotations

import io
import math
import os
import random

from PIL import Image, ImageDraw

from corpus_builder import fonts
from corpus_builder.ooxml import (
    FontFamily,
    WordDocument,
    anchored_shape,
    footnote_reference,
    inline_picture,
    paragraph,
    run,
    section_properties,
    table,
)
from corpus_builder.text import paragraph_text

LETTER = (612.0, 792.0)


def _families(*roles: str) -> list[FontFamily]:
    """every style of each role as its own embedded family."""
    found = []
    for role in roles:
        for style in fonts.FAMILIES[role][1]:
            with open(fonts.style_family_file(role, style), "rb") as handle:
                found.append(FontFamily(fonts.style_family_name(role, style), {"regular": handle.read()}))
    return found


def syllabus(out_dir: str) -> tuple[str, dict]:
    doc = WordDocument(fonts=_families("serif", "sans"), body_font="Crimson Text", body_size=11)
    b = doc.body
    b.append(paragraph(run("PHYS 201: Waves, Optics and Measurement"), style="Title"))
    b.append(paragraph(run("Spring term, Tuesdays and Thursdays 10:30 to 11:45, Room 2.14", italic=True)))
    b.append(paragraph(run("Course description"), style="Heading1"))
    b.append(paragraph(run(paragraph_text(1, 6)), align="both"))
    b.append(paragraph(run("Learning outcomes"), style="Heading1"))
    outcomes = [
        "Describe wave motion with the language of frequency, phase and amplitude.",
        "Design a simple optics experiment and estimate its uncertainty.",
        "Write a lab report that a classmate could reproduce.",
        "Read a research article and summarise its main claim in one paragraph.",
    ]
    for item in outcomes:
        b.append(paragraph(run(item), numbering=(1, 0), after=2))
    b.append(paragraph(run("Required materials"), style="Heading2"))
    for item in ["Lab notebook with numbered pages", "Scientific calculator", "Safety glasses"]:
        b.append(paragraph(run(item), numbering=(2, 0), after=2))
    b.append(paragraph(run("Weekly schedule"), style="Heading1"))
    rows = [["Week", "Date", "Topic", "Reading"]]
    topics = ["Oscillations", "Travelling waves", "Superposition", "Interference", "Diffraction", "Polarisation",
              "Geometric optics", "Lenses and mirrors", "Measurement error", "Review"]  # fmt: skip
    for week, topic in enumerate(topics, start=1):
        rows.append(
            [
                str(week),
                f"Feb {2 + 7 * (week - 1) % 28:02d}",
                topic,
                f"Chapter {week + 2}, pp. {10 * week}-{10 * week + 14}",
            ]
        )
    b.append(table(rows, [42, 70, 150, 170]))
    b.append(paragraph(run("Assessment"), style="Heading1", before=12))
    b.append(paragraph(run(paragraph_text(2, 5)), align="both"))
    for item in ["Weekly problem sets: 25%", "Lab reports: 35%", "Midterm exam: 15%", "Final exam: 25%"]:
        b.append(paragraph(run(item), numbering=(1, 0), after=2))
    b.append(paragraph(run("Policies"), style="Heading1"))
    for seed in (3, 4, 5):
        b.append(paragraph(run(paragraph_text(seed, 5)), align="both"))
    doc.final_section = section_properties(*LETTER, margin_pt=72)
    path = doc.save(os.path.join(out_dir, "syllabus.docx"))
    return path, {
        "class": "syllabus",
        "producer": "Microsoft Word (ExportAsFixedFormat) from hand-written OOXML",
        "features": ["headings", "bullet list", "numbered list", "table with rules", "justified text", "2 fonts"],
        "known_phrase": "Weekly schedule",
        "known_sentence": outcomes[1],
    }


def research_paper(out_dir: str) -> tuple[str, dict]:
    doc = WordDocument(fonts=_families("serif", "sans", "math"), body_font="Crimson Text", body_size=10)
    b = doc.body
    b.append(paragraph(run("Measuring Drift in Low-Cost Optical Sensors"), style="Title", align="center"))
    b.append(paragraph(run("A. Student", font="Lato") + run("1", font="Lato", vert="superscript") +
                       run(", B. Partner", font="Lato") + run("2", font="Lato", vert="superscript"), align="center"))  # fmt: skip
    abstract = run("Abstract. ", bold=True) + run(paragraph_text(10, 5), italic=True)
    single_column = section_properties(*LETTER, margin_pt=60, columns=1)
    b.append(paragraph(abstract, align="both", section=single_column))
    b.append(paragraph(run("1 Introduction"), style="Heading2"))
    b.append(paragraph(run(paragraph_text(11, 7), kern=True) + footnote_reference(doc.add_footnote(run(paragraph_text(30, 1)))) +
                       run(" " + paragraph_text(12, 3), kern=True), align="both"))  # fmt: skip
    b.append(
        paragraph(
            run(paragraph_text(13, 6), kern=True) + run("[3]", vert="superscript") + run(" " + paragraph_text(14, 4)),
            align="both",
        )
    )
    b.append(paragraph(run("2 Method"), style="Heading2"))
    b.append(paragraph(run(paragraph_text(15, 6), kern=True), align="both"))
    equation = (
        run("E", italic=True) + run("(", italic=False) + run("t", italic=True) + run(") = ")
        + run("∑", font="Noto Sans Math") + run("i", italic=True, vert="subscript") + run(" ")
        + run("α", font="Noto Sans Math") + run("i", italic=True, vert="subscript")
        + run("x", italic=True) + run("i", italic=True, vert="subscript") + run("2", vert="superscript")
        + run(" ≤ ", font="Noto Sans Math") + run("∫", font="Noto Sans Math")
        + run(" f(x) dx + ", italic=True) + run("ε", font="Noto Sans Math") + run("      (1)")
    )  # fmt: skip
    b.append(paragraph(equation, align="center", before=6, after=6))
    b.append(
        paragraph(
            run(paragraph_text(16, 5), kern=True) + footnote_reference(doc.add_footnote(run(paragraph_text(31, 1)))),
            align="both",
        )
    )
    b.append(paragraph(run("3 Results"), style="Heading2"))
    for seed in (17, 18, 19):
        b.append(paragraph(run(paragraph_text(seed, 7), kern=True), align="both"))
    b.append(paragraph(run("4 Discussion"), style="Heading2"))
    for seed in (20, 21, 22, 23):
        b.append(paragraph(run(paragraph_text(seed, 7), kern=True), align="both"))
    b.append(paragraph(run("References"), style="Heading2"))
    for index in range(1, 6):
        b.append(
            paragraph(
                run(f"[{index}] Author {index}, Title of a cited work, Journal {index}, 20{10 + index}.", size=8.5),
                after=2,
            )
        )
    doc.final_section = section_properties(*LETTER, margin_pt=60, columns=2, continuous=True)
    path = doc.save(os.path.join(out_dir, "research_paper.docx"))
    return path, {
        "class": "research paper",
        "producer": "Microsoft Word (ExportAsFixedFormat) from hand-written OOXML",
        "features": [
            "two columns",
            "justified text",
            "kerning on",
            "footnotes",
            "superscripts",
            "equation with math symbols",
            "3 fonts",
        ],
        "known_phrase": "2 Method",
        "known_sentence": paragraph_text(15, 1),
    }


def _plot_png(width: int, height: int) -> bytes:
    """a plot-like picture with no text, so every glyph on the page is real text."""
    image = Image.new("RGB", (width, height), "white")
    draw = ImageDraw.Draw(image)
    draw.rectangle([40, 10, width - 10, height - 40], outline=(60, 60, 60), width=2)
    points = [
        (40 + i * (width - 50) / 60, height - 40 - (height - 60) * (0.5 + 0.4 * math.sin(i / 6))) for i in range(61)
    ]
    draw.line(points, fill=(30, 90, 200), width=3)
    rng = random.Random(4)
    for x, y in points[::3]:
        y += rng.uniform(-12, 12)
        draw.ellipse([x - 4, y - 4, x + 4, y + 4], outline=(200, 60, 40), width=2)
    buffer = io.BytesIO()
    image.save(buffer, "PNG")
    return buffer.getvalue()


def _photo_jpeg(width: int, height: int) -> bytes:
    image = Image.new("RGB", (width, height))
    pixels = image.load()
    rng = random.Random(9)
    for y in range(height):
        for x in range(width):
            noise = rng.randint(-12, 12)
            pixels[x, y] = (min(255, max(0, 90 + x // 4 + noise)), min(255, max(0, 120 + y // 3 + noise)), 160)
    buffer = io.BytesIO()
    image.save(buffer, "JPEG", quality=88)
    return buffer.getvalue()


def lab_report(out_dir: str) -> tuple[str, dict]:
    doc = WordDocument(fonts=_families("serif", "sans"), body_font="Crimson Text", body_size=11)
    b = doc.body
    b.append(paragraph(run("Lab 4: Damped Oscillations"), style="Title"))
    b.append(paragraph(run("Group 7, submitted March 12", italic=True)))
    b.append(paragraph(run("Aim"), style="Heading1"))
    b.append(paragraph(run(paragraph_text(40, 4)), align="both"))
    b.append(paragraph(run("Setup"), style="Heading1"))
    photo = doc.add_media("setup.jpeg", _photo_jpeg(480, 300))
    b.append(paragraph(inline_picture(photo, 288, 180, 101, "setup photo"), align="center", after=2))
    b.append(
        paragraph(run("Figure 1. The pendulum and the optical gate used for timing."), style="Caption", align="center")
    )
    b.append(paragraph(run(paragraph_text(41, 5)), align="both"))
    b.append(paragraph(run("Results"), style="Heading1"))
    plot = doc.add_media("plot.png", _plot_png(640, 360))
    b.append(paragraph(inline_picture(plot, 320, 180, 102, "amplitude plot"), align="center", after=2))
    b.append(
        paragraph(run("Figure 2. Amplitude against time with the fitted envelope."), style="Caption", align="center")
    )
    # a bar chart drawn with shapes, the way students draw one by hand in word
    chart = [anchored_shape(201, 40, 4, 0.01, 120, geometry="line", line="333333", line_pt=1),
             anchored_shape(202, 40, 124, 300, 0.01, geometry="line", line="333333", line_pt=1)]  # fmt: skip
    for index, value in enumerate([70, 95, 54, 110, 82]):
        chart.append(
            anchored_shape(
                210 + index,
                60 + index * 56,
                124 - value,
                36,
                value,
                fill=["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5"][index],
            )
        )
    b.append(paragraph("".join(chart), after=0) + paragraph(run(" "), after=118))
    b.append(
        paragraph(run("Figure 3. Mean period in each trial, drawn as a bar chart."), style="Caption", align="center")
    )
    rows = [["Trial", "Period (s)", "Damping (1/s)"]] + [
        [str(i), f"{1.92 + 0.013 * i:.3f}", f"{0.041 + 0.002 * i:.3f}"] for i in range(1, 6)
    ]
    b.append(table(rows, [60, 100, 110]))
    b.append(paragraph(run("Discussion"), style="Heading1", before=10))
    for seed in (42, 43):
        b.append(paragraph(run(paragraph_text(seed, 6)), align="both"))
    doc.final_section = section_properties(*LETTER, margin_pt=72)
    path = doc.save(os.path.join(out_dir, "lab_report.docx"))
    return path, {
        "class": "lab report",
        "producer": "Microsoft Word (ExportAsFixedFormat) from hand-written OOXML",
        "features": ["jpeg photo", "png plot", "bar chart drawn with shapes", "captions", "table with rules"],
        "known_phrase": "Damped Oscillations",
        "known_sentence": "Figure 2. Amplitude against time with the fitted envelope.",
    }


BUILDERS = {"syllabus": syllabus, "research_paper": research_paper, "lab_report": lab_report}
