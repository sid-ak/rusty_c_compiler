#!/usr/bin/env python3
"""Generate one architecture diagram per phase, with that phase's parts outlined in red.

Each `docs/assets/phase-N.svg` is `docs/assets/architecture.svg` plus red outlines around the
boxes that phase builds, so the phase explanations can show where the phase sits in the finished
compiler. The copies are generated rather than hand-edited so that a change to the architecture
diagram reaches every phase diagram, and `--check` fails the docs build when one is out of date.

Usage: scripts/phase_diagrams.py          write the phase diagrams
       scripts/phase_diagrams.py --check  exit 1 if any committed diagram differs
"""

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "docs" / "assets" / "architecture.svg"
TITLE = "<title>Rusty C Compiler architecture</title>"

# How far an outline sits outside the box it surrounds, in SVG units.
PADDING = 6

# Each phase's highlighted boxes as (x, y, width, height) of the box in architecture.svg; the
# outline is drawn PADDING outside it. Coordinates are copied from the source diagram's rects.
PHASES = {
    1: (
        "Foundation, diagnostics, and the lexer",
        [(196, 104, 140, 60), (196, 200, 484, 40), (594, 396, 150, 44)],
    ),
    2: ("The AST and the recursive-descent parser", [(368, 104, 140, 60)]),
    3: ("Semantic analysis", [(540, 104, 140, 60)]),
    4: ("ARM64 code generation and the driver", [(712, 104, 312, 60), (8, 268, 1024, 196)]),
    5: ("Differential testing, fuzzing, and system acceptance", [(8, 480, 1024, 112)]),
}


def outline(box):
    """The SVG rect drawing a red outline PADDING outside `box`."""
    x, y, width, height = box
    return (
        f'  <rect x="{x - PADDING}" y="{y - PADDING}" width="{width + 2 * PADDING}" '
        f'height="{height + 2 * PADDING}" rx="10" fill="none" stroke="#dc2626" stroke-width="3"/>\n'
    )


def render(source, number, name, boxes):
    """`source` with phase `number`'s title and its `boxes` outlined."""
    if TITLE not in source or "</svg>" not in source:
        raise SystemExit(f"error: {SOURCE} no longer has the title or closing tag this script edits")

    title = f"<title>Phase {number} — {name}: its parts of the architecture outlined in red</title>"
    outlines = f"  <!-- Phase {number} highlight -->\n" + "".join(outline(box) for box in boxes)
    header = f"<!-- Generated from architecture.svg by scripts/phase_diagrams.py. Do not edit. -->\n"

    body = source.replace(TITLE, title, 1).replace("</svg>", outlines + "</svg>", 1)
    declaration, rest = body.split("\n", 1)
    return f"{declaration}\n{header}{rest}"


def main(argv):
    """Write every phase diagram, or with `--check` report the ones that are out of date."""
    check = argv[1:] == ["--check"]
    source = SOURCE.read_text(encoding="utf-8")
    stale = []

    for number, (name, boxes) in PHASES.items():
        target = SOURCE.with_name(f"phase-{number}.svg")
        expected = render(source, number, name, boxes)
        current = target.read_text(encoding="utf-8") if target.exists() else None

        if current == expected:
            continue
        if check:
            stale.append(target.relative_to(ROOT))
        else:
            target.write_text(expected, encoding="utf-8")
            print(f"wrote {target.relative_to(ROOT)}")

    if stale:
        print("error: phase diagrams are out of date; run scripts/phase_diagrams.py:", file=sys.stderr)
        for path in stale:
            print(f"  {path}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
