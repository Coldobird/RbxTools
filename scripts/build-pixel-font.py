"""Rebuild the bundled RBX Pixel derivative (python -m pip install fonttools brotli)."""
from pathlib import Path
import math
import shutil
import unicodedata

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.ttProgram import Program

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "node_modules/@fontsource/press-start-2p"
DEST = ROOT / "assets/fonts"
GRID = 125  # One original design pixel, in a 1000-unit em.
PAIRS = "AV AW AY AT VA WA YA Ta Te To Tr Tu Ty Yo Ya Ye Wo Wa We Fo Fa Po Pa LT LV LY".split()


def row_edges(glyph, glyf):
    """Sample the original square-pixel outlines to avoid touching kerned pairs."""
    coordinates, ends, _ = glyph.getCoordinates(glyf)
    contours, start = [], 0
    for end in ends:
        contours.append(list(coordinates[start:end + 1]))
        start = end + 1
    rows = {}
    for row in range(math.floor(glyph.yMin / GRID), math.ceil(glyph.yMax / GRID)):
        y = (row + 0.5) * GRID
        crossings = []
        for contour in contours:
            for (x1, y1), (x2, y2) in zip(contour, contour[1:] + contour[:1]):
                if (y1 <= y < y2) or (y2 <= y < y1):
                    crossings.append(x1 + (y - y1) * (x2 - x1) / (y2 - y1))
        if crossings:
            rows[row] = (min(crossings), max(crossings))
    return rows


DEST.mkdir(parents=True, exist_ok=True)
shutil.copyfile(SOURCE / "LICENSE", DEST / "OFL.txt")
# Also ship the notice alongside the application web assets.
notice = ROOT / "public/fonts/OFL.txt"
notice.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(SOURCE / "LICENSE", notice)

for source in sorted((SOURCE / "files").glob("*-400-normal.woff2")):
    font = TTFont(source, recalcTimestamp=False)
    cmap = font.getBestCmap()
    digits = {cmap[c] for c in range(ord("0"), ord("9") + 1) if c in cmap}
    marks = {name for code, name in cmap.items() if unicodedata.category(chr(code)).startswith("M")}
    spaces = {cmap[c] for c in (0x20, 0xA0) if c in cmap}
    shifts = {}
    for name in font.getGlyphOrder():
        advance, bearing = font["hmtx"][name]
        glyph = font["glyf"][name]
        if name in spaces:
            font["hmtx"][name] = (GRID * 4, bearing)
        elif advance > 0 and name not in digits | marks and glyph.numberOfContours != 0:
            glyph.recalcBounds(font["glyf"])
            shifts[name] = glyph.xMin
            font["hmtx"][name] = (glyph.xMax - glyph.xMin + GRID, 0)
    # Translate existing points without rebuilding contours: point numbering
    # and composite structure stay intact for the pixel-grid hint programs.
    for name in font.getGlyphOrder():
        glyph = font["glyf"][name]
        shift = shifts.get(name, 0)
        if glyph.isComposite():
            for component in glyph.components:
                assert not hasattr(component, "transform"), "Handle scaled components explicitly"
                if hasattr(component, "x"):
                    component.x += shifts.get(component.glyphName, 0) - shift
                else:
                    assert shift == 0 and shifts.get(component.glyphName, 0) == 0
        elif glyph.numberOfContours > 0:
            glyph.coordinates.translate((-shift, 0))
            # Replace the source's optical autohints with direct pixel-grid
            # snapping. Those hints reshape strokes intended for smooth type.
            instructions = ["RTG[ ]"]
            for axis in (1, 0):
                instructions.append(f"SVTCA[{axis}]")
                for point in range(len(glyph.coordinates)):
                    instructions.extend(["PUSHW[ ]", str(point), "MDAP[1]"])
            glyph.program = Program()
            glyph.program.fromAssembly(instructions)

    font["maxp"].maxSizeOfInstructions = max(
        len(g.program.getBytecode()) for g in font["glyf"].glyphs.values() if hasattr(g, "program")
    )
    kerns = []
    for pair in PAIRS:
        if not all(ord(c) in cmap for c in pair):
            continue
        left, right = [cmap[ord(c)] for c in pair]
        for name in (left, right):
            font["glyf"][name].recalcBounds(font["glyf"])
        a, b = [row_edges(font["glyf"][name], font["glyf"]) for name in (left, right)]
        gap = min(font["hmtx"][left][0] + b[row][0] - a[row][1] for row in a.keys() & b.keys())
        if gap >= GRID * 2:
            kerns.append(f"pos {left} {right} -{GRID};")
    if kerns:
        addOpenTypeFeaturesFromString(font, "feature kern {\n" + "\n".join(kerns) + "\n} kern;", tables=["GPOS"])
    font["post"].isFixedPitch = 0
    font["OS/2"].panose.bProportion = 0
    font["OS/2"].recalcAvgCharWidth(font)
    # Small pixel lettering should grid-fit rather than request grayscale
    # smoothing. Larger display sizes may still use the original smoothing.
    font["gasp"].gaspRange = {24: 1, 65535: 15}
    names = {1: "RBX Pixel", 2: "Regular", 3: "RBX Pixel Regular 1.0",
             4: "RBX Pixel Regular", 6: "RBXPixel-Regular", 16: "RBX Pixel", 17: "Regular"}
    for record in font["name"].names:
        if record.nameID in names:
            record.string = names[record.nameID].encode(record.getEncoding())
    font["name"].setName("RBX Pixel: proportional spacing derivative of Press Start 2P. SIL OFL 1.1.", 10, 3, 1, 0x409)
    output = DEST / source.name.replace("press-start-2p", "rbx-pixel")
    font.save(output)
    # Reopen serialized output to catch malformed glyph or layout tables.
    check = TTFont(output)
    assert check.getBestCmap() == cmap
    assert all(check["hmtx"][name][0] == 1000 for name in digits)
    original = TTFont(source)
    for tag in ("GSUB", "GDEF", "fpgm", "prep", "cvt "):
        if tag in original:
            assert check[tag].compile(check) == original[tag].compile(original)
    for name in original.getGlyphOrder():
        if original["glyf"][name].isComposite() and hasattr(original["glyf"][name], "program"):
            assert check["glyf"][name].program.getBytecode() == original["glyf"][name].program.getBytecode()
        before, _, _ = original["glyf"][name].getCoordinates(original["glyf"])
        after, _, _ = check["glyf"][name].getCoordinates(check["glyf"])
        # The design may move horizontally, but every stroke stays identical.
        if before:
            before_left = min(x for x, _ in before)
            after_left = min(x for x, _ in after)
            assert [(x - before_left, y) for x, y in before] == [(x - after_left, y) for x, y in after], name
    print(f"{output.name}: {len(shifts)} respaced glyphs, {len(kerns)} kern pairs; direct pixel-grid hints")
