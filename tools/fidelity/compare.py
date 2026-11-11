"""comparing a source pdf with the pdf word exported: pixels and glyph positions."""

from __future__ import annotations

import difflib
import os

import numpy as np
from PIL import Image
from skimage.metrics import structural_similarity

from pdf_analysis import PlacedChar

# the target the whole experiment is about: 1/40 pt
TOLERANCE_PT = 0.025
# pixel differences below this are antialiasing noise rather than a moved edge
PIXEL_NOISE_LEVEL = 32


def _pad_to_same_size(a: np.ndarray, b: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    height = max(a.shape[0], b.shape[0])
    width = max(a.shape[1], b.shape[1])

    def pad(image):
        out = np.full((height, width), 255, dtype=np.uint8)
        out[: image.shape[0], : image.shape[1]] = image
        return out

    return pad(a), pad(b)


def _heatmap(source: np.ndarray, candidate: np.ndarray, diff: np.ndarray) -> Image.Image:
    """faint source in gray, pixels only in the source in blue, only in the candidate in red.

    seeing which side a stroke sits on tells shifts apart from substitutions."""
    rgb = np.stack([source, source, source], axis=-1).astype(np.float32)
    rgb = 255 - (255 - rgb) * 0.25
    strong = diff > PIXEL_NOISE_LEVEL
    darker_in_source = strong & (source < candidate)
    darker_in_candidate = strong & (candidate < source)
    rgb[darker_in_source] = (30, 90, 255)
    rgb[darker_in_candidate] = (230, 30, 30)
    return Image.fromarray(rgb.astype(np.uint8), "RGB")


def compare_rasters(source_pages: list[np.ndarray], candidate_pages: list[np.ndarray], heatmap_prefix: str) -> dict:
    pages = []
    for index in range(max(len(source_pages), len(candidate_pages))):
        if index >= len(source_pages) or index >= len(candidate_pages):
            pages.append({"page": index, "missing_in": "source" if index >= len(source_pages) else "candidate"})
            continue
        source, candidate = _pad_to_same_size(source_pages[index], candidate_pages[index])
        diff = np.abs(source.astype(np.int16) - candidate.astype(np.int16)).astype(np.uint8)
        ssim = structural_similarity(source, candidate, data_range=255)
        heatmap_path = f"{heatmap_prefix}_page{index + 1}.png"
        _heatmap(source, candidate, diff).save(heatmap_path)
        ink = (source < 255 - PIXEL_NOISE_LEVEL) | (candidate < 255 - PIXEL_NOISE_LEVEL)
        pages.append(
            {
                "page": index,
                "mean_abs_diff": float(diff.mean()),
                "max_abs_diff": int(diff.max()),
                "pixels_over_noise": int((diff > PIXEL_NOISE_LEVEL).sum()),
                "ink_pixels": int(ink.sum()),
                "ssim": float(ssim),
                "heatmap": os.path.basename(heatmap_path),
            }
        )
    return {"pages": pages}


def match_chars(source: list[PlacedChar], candidate: list[PlacedChar]) -> list[tuple[PlacedChar, PlacedChar]]:
    """pair characters in reading order.

    a plain zip goes wrong as soon as word drops or adds one character, so the
    sequences are aligned on their text first and only equal runs are paired."""
    source_text = [c.char for c in source]
    candidate_text = [c.char for c in candidate]
    matcher = difflib.SequenceMatcher(a=source_text, b=candidate_text, autojunk=False)
    pairs = []
    for block in matcher.get_matching_blocks():
        for k in range(block.size):
            pairs.append((source[block.a + k], candidate[block.b + k]))
    return pairs


def _stats(values: np.ndarray) -> dict:
    if values.size == 0:
        return {"max": None, "mean": None, "p99": None}
    return {
        "max": float(values.max()),
        "mean": float(values.mean()),
        "p99": float(np.percentile(values, 99)),
    }


def compare_glyphs(source: list[PlacedChar], candidate: list[PlacedChar]) -> dict:
    pairs = match_chars(source, candidate)
    same_page = [(s, c) for s, c in pairs if s.page == c.page]
    dx = np.array([c.origin_x - s.origin_x for s, c in same_page])
    dy = np.array([c.origin_y - s.origin_y for s, c in same_page])
    distance = np.hypot(dx, dy)
    size_delta = np.array([c.font_size - s.font_size for s, c in same_page])
    return {
        "source_chars": len(source),
        "candidate_chars": len(candidate),
        "matched": len(pairs),
        "matched_on_wrong_page": len(pairs) - len(same_page),
        "origin_distance_pt": _stats(distance),
        "abs_dx_pt": _stats(np.abs(dx)),
        "abs_dy_pt": _stats(np.abs(dy)),
        "mean_dx_pt": float(dx.mean()) if dx.size else None,
        "mean_dy_pt": float(dy.mean()) if dy.size else None,
        "within_tolerance_fraction": float((distance <= TOLERANCE_PT).mean()) if distance.size else None,
        "abs_font_size_delta_max": float(np.abs(size_delta).max()) if size_delta.size else None,
    }


def per_char_deltas(source: list[PlacedChar], candidate: list[PlacedChar]) -> list[dict]:
    """every matched pair, for experiments that want to look for drift patterns."""
    rows = []
    for s, c in match_chars(source, candidate):
        rows.append(
            {
                "char": s.char,
                "page": s.page,
                "candidate_page": c.page,
                "source_x": s.origin_x,
                "source_y": s.origin_y,
                "dx": c.origin_x - s.origin_x,
                "dy": c.origin_y - s.origin_y,
            }
        )
    return rows
