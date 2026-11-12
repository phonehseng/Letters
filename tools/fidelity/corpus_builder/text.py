"""deterministic filler prose, so every corpus run produces identical documents."""

from __future__ import annotations

import random

SUBJECTS = [
    "The experiment", "Each student", "The second sample", "This approach", "Our model", "The measured signal",
    "The control group", "A careful reading", "The proposed method", "The final report", "Every lecture",
    "The reaction rate", "The survey data", "This assignment", "The reference curve",
]  # fmt: skip
VERBS = [
    "suggests", "confirms", "requires", "reduces", "explains", "compares", "follows", "improves", "measures",
    "describes", "supports", "predicts", "affects", "highlights", "limits",
]  # fmt: skip
OBJECTS = [
    "the overall accuracy of the results", "a clear pattern across all trials", "the role of temperature",
    "several independent observations", "the effect of sample size", "a small but steady drift",
    "the expected behaviour at low pressure", "the main sources of uncertainty", "the weekly reading list",
    "a consistent trend in the second half", "the limits of the linear model", "the structure of the argument",
    "the efficiency of the new design", "the variance between groups", "the deadlines for every submission",
]  # fmt: skip
TAILS = [
    "", "", " in most cases", " when the input is noisy", " within the stated error", " for the full data set",
    " after the first week", " under controlled conditions", " as shown in the table", " with few exceptions",
]  # fmt: skip


def sentence(rng: random.Random) -> str:
    return f"{rng.choice(SUBJECTS)} {rng.choice(VERBS)} {rng.choice(OBJECTS)}{rng.choice(TAILS)}."


def paragraph_text(seed: int, sentences: int = 5) -> str:
    rng = random.Random(seed)
    return " ".join(sentence(rng) for _ in range(sentences))
