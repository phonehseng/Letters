"""makes the round 4 fidelity modules importable from this folder, with their data read from elsewhere.

the prototype lives one folder up (tools/fidelity) and imports its modules by
bare name, so that folder goes on sys.path. its fonts and corpus were built in
the fidelity worktree (C:\\Letters-fidelity), which is only read here; the
calibration cache is a copy kept in this folder's out\\, so nothing outside
this folder is written.
"""

from __future__ import annotations

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FIDELITY_DIR = os.path.dirname(HERE)
WORKTREE_FIDELITY_DIR = r"C:\Letters-fidelity\tools\fidelity"
OUT_DIR = os.path.join(HERE, "out")
CORPUS_DIR = os.path.join(OUT_DIR, "corpus")

if FIDELITY_DIR not in sys.path:
    sys.path.insert(0, FIDELITY_DIR)


def _point_shared_data_here() -> None:
    # the downloaded open fonts exist only in the worktree; pointing the
    # modules there avoids downloading them again into a folder we do not own
    import test_fonts
    from corpus_builder import fonts as corpus_fonts
    from prototype import calibration

    worktree_fonts = os.path.join(WORKTREE_FIDELITY_DIR, "fonts")
    if os.path.isdir(worktree_fonts):
        test_fonts.FONT_DIR = worktree_fonts
        test_fonts.BUILD_DIR = os.path.join(OUT_DIR, "font_build")
        corpus_fonts.FONT_DIR = worktree_fonts
        corpus_fonts.BUILD_DIR = os.path.join(worktree_fonts, "build", "corpus")
    calibration.CACHE_DIR = os.path.join(OUT_DIR, "calibration_cache")


_point_shared_data_here()
