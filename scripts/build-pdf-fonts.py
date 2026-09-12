#!/usr/bin/env python3
"""Rebuild the fonts the lesson PDF embeds.

The desk's screen fonts are variable WOFF2 subsets from Fontsource, split by
script and missing the arrows, check marks and operators lessons use. The PDF
needs static instances with a symbol-rich character set, so this script takes
the variable fonts from the Google Fonts repository, pins one weight each, and
subsets them to Latin plus the symbol blocks, as WOFF (zlib, which pdfmake's
font parser reads without extra decoders).

Requires fontTools:  pip install fonttools brotli

    python3 scripts/build-pdf-fonts.py            # downloads into a temp dir
"""
from __future__ import annotations

import os
import sys
import tempfile
import urllib.request

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

REPO = "https://github.com/google/fonts/raw/main/ofl/"
SOURCES = {
    "Inter-VF.ttf": "inter/Inter%5Bopsz%2Cwght%5D.ttf",
    "Inter-Italic-VF.ttf": "inter/Inter-Italic%5Bopsz%2Cwght%5D.ttf",
    "JetBrainsMono-VF.ttf": "jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf",
    "Fraunces-VF.ttf": "fraunces/Fraunces%5BSOFT%2CWONK%2Copsz%2Cwght%5D.ttf",
}
# Latin, Latin Extended, IPA, Greek, punctuation, currency, letterlike,
# arrows, mathematical operators, technical, box drawing, shapes, dingbats.
UNICODES = (
    "U+0000-024F,U+02B0-02FF,U+0370-03FF,U+1E00-1EFF,U+2000-206F,U+20A0-20CF,"
    "U+2100-214F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2500-257F,U+25A0-25FF,"
    "U+2600-26FF,U+2700-27BF,U+2B00-2BFF,U+FB00-FB06,U+FEFF,U+FFFD"
)
# (source, output, axis values, rename via STAT, keep layout features)
# JetBrains Mono's programming ligatures (:=, <=, ||) are substituted by the
# PDF engine's shaper into glyphs whose outlines it then cannot read, so the
# mono face ships without its layout features; code prints glyph for glyph.
# Fraunces' STAT table lacks these named values, so its names stay as is.
FACES = [
    ("Inter-VF.ttf", "inter-regular.woff", {"wght": 400, "opsz": 14}, True, True),
    ("Inter-VF.ttf", "inter-bold.woff", {"wght": 700, "opsz": 14}, True, True),
    ("Inter-Italic-VF.ttf", "inter-italic.woff", {"wght": 400, "opsz": 14}, True, True),
    ("Inter-Italic-VF.ttf", "inter-bold-italic.woff", {"wght": 700, "opsz": 14}, True, True),
    ("JetBrainsMono-VF.ttf", "jetbrains-mono-regular.woff", {"wght": 400}, True, False),
    ("JetBrainsMono-VF.ttf", "jetbrains-mono-bold.woff", {"wght": 700}, True, False),
    ("Fraunces-VF.ttf", "fraunces-medium.woff", {"wght": 500, "opsz": 24, "SOFT": 0, "WONK": 0}, False, True),
    ("Fraunces-VF.ttf", "fraunces-semibold.woff", {"wght": 600, "opsz": 24, "SOFT": 0, "WONK": 0}, False, True),
]


def build(source: str, out: str, axes: dict, rename: bool, features: bool) -> None:
    instance = instancer.instantiateVariableFont(TTFont(source), axes, inplace=False, updateFontNames=rename)
    staged = out + ".tmp.ttf"
    instance.save(staged)
    options = subset.Options()
    options.flavor = "woff"
    options.layout_features = ["*"] if features else []
    options.name_IDs = ["*"]
    options.notdef_outline = True
    options.desubroutinize = True
    subsetter = subset.Subsetter(options)
    font = TTFont(staged)
    subsetter.populate(unicodes=subset.parse_unicodes(UNICODES))
    subsetter.subset(font)
    font.flavor = "woff"
    font.save(out)
    os.remove(staged)
    print(f"{out}: {os.path.getsize(out)} bytes")


def main() -> int:
    target = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "static", "fonts", "pdf")
    os.makedirs(target, exist_ok=True)
    with tempfile.TemporaryDirectory() as work:
        for name, path in SOURCES.items():
            urllib.request.urlretrieve(REPO + path, os.path.join(work, name))
        for source, out, axes, rename, features in FACES:
            build(os.path.join(work, source), os.path.join(target, out), axes, rename, features)
    return 0


if __name__ == "__main__":
    sys.exit(main())
