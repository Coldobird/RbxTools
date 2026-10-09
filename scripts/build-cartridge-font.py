"""Build the original RBX Cartridge pixel alphabet. No source font is read."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.ttProgram import Program
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

# Each 1 is one solid design pixel. Capitals are 7 high; lowercase has a
# five-pixel x-height, with distinct ascenders and descenders. Width is optical.
PATTERNS = {
    "A": "01110/11011/10001/10001/11111/10001/10001",
    "B": "11110/10001/10001/11110/10001/10001/11110",
    "C": "01111/10000/10000/10000/10000/10000/01111",
    "D": "11110/10001/10001/10001/10001/10001/11110",
    "E": "11111/10000/10000/11110/10000/10000/11111",
    "F": "11111/10000/10000/11110/10000/10000/10000",
    "G": "01111/10000/10000/10111/10001/10001/01111",
    "H": "10001/10001/10001/11111/10001/10001/10001",
    "I": "111/010/010/010/010/010/111",
    "J": "00111/00010/00010/00010/10010/10010/01100",
    "K": "10001/10010/10100/11000/10100/10010/10001",
    "L": "10000/10000/10000/10000/10000/10000/11111",
    "M": "10001/11011/10101/10101/10001/10001/10001",
    "N": "10001/11001/11001/10101/10011/10011/10001",
    "O": "01110/10001/10001/10001/10001/10001/01110",
    "P": "11110/10001/10001/11110/10000/10000/10000",
    "Q": "01110/10001/10001/10001/10101/10010/01101",
    "R": "11110/10001/10001/11110/10100/10010/10001",
    "S": "01111/10000/10000/01110/00001/00001/11110",
    "T": "11111/00100/00100/00100/00100/00100/00100",
    "U": "10001/10001/10001/10001/10001/10001/01110",
    "V": "10001/10001/10001/10001/01010/01010/00100",
    "W": "10001/10001/10001/10101/10101/11011/10001",
    "X": "10001/10001/01010/00100/01010/10001/10001",
    "Y": "10001/10001/01010/00100/00100/00100/00100",
    "Z": "11111/00001/00010/00100/01000/10000/11111",
    "a": "00000/00000/01110/00001/01111/10001/01111",
    "b": "10000/10000/11110/10001/10001/10001/11110",
    "c": "0000/0000/0111/1000/1000/1000/0111",
    "d": "00001/00001/01111/10001/10001/10001/01111",
    "e": "00000/00000/01110/10001/11111/10000/01111",
    "f": "0011/0100/1110/0100/0100/0100/0100",
    "g": "00000/00000/01111/10001/10001/01111/00001/11110",
    "h": "10000/10000/11110/10001/10001/10001/10001",
    "i": "01/00/11/01/01/01/01",
    "j": "001/000/011/001/001/001/001/110",
    "k": "1000/1000/1001/1010/1100/1010/1001",
    "l": "11/01/01/01/01/01/01",
    "m": "00000/00000/11010/10101/10101/10101/10101",
    "n": "00000/00000/11110/10001/10001/10001/10001",
    "o": "00000/00000/01110/10001/10001/10001/01110",
    "p": "00000/00000/11110/10001/10001/11110/10000/10000",
    "q": "00000/00000/01111/10001/10001/01111/00001/00001",
    "r": "0000/0000/1011/1100/1000/1000/1000",
    "s": "0000/0000/0111/1000/0110/0001/1110",
    "t": "0100/0100/1110/0100/0100/0100/0011",
    "u": "00000/00000/10001/10001/10001/10001/01111",
    "v": "00000/00000/10001/10001/10001/01010/00100",
    "w": "00000/00000/10001/10001/10101/10101/01010",
    "x": "00000/00000/10001/01010/00100/01010/10001",
    "y": "00000/00000/10001/10001/10001/01111/00001/11110",
    "z": "0000/0000/1111/0001/0110/1000/1111",
    "0": "01110/10001/10011/10101/11001/10001/01110",
    "1": "00100/01100/00100/00100/00100/00100/01110",
    "2": "01110/10001/00001/00010/00100/01000/11111",
    "3": "11110/00001/00001/01110/00001/00001/11110",
    "4": "00010/00110/01010/10010/11111/00010/00010",
    "5": "11111/10000/10000/11110/00001/00001/11110",
    "6": "01110/10000/10000/11110/10001/10001/01110",
    "7": "11111/00001/00010/00100/01000/01000/01000",
    "8": "01110/10001/10001/01110/10001/10001/01110",
    "9": "01110/10001/10001/01111/00001/00001/01110",
    " ": "000/000/000/000/000/000/000",
    ".": "0/0/0/0/0/0/1", ",": "00/00/00/00/00/00/01/10",
    ":": "0/0/1/0/0/1/0", ";": "00/00/01/00/00/01/10",
    "!": "1/1/1/1/1/0/1", "?": "01110/10001/00001/00110/00100/00000/00100",
    "'": "1/1/0/0/0/0/0", '"': "101/101/000/000/000/000/000",
    "-": "000/000/000/111/000/000/000", "_": "00000/00000/00000/00000/00000/00000/11111",
    "/": "00001/00001/00010/00100/01000/10000/10000",
    "\\": "10000/10000/01000/00100/00010/00001/00001",
    "(": "001/010/100/100/100/010/001", ")": "100/010/001/001/001/010/100",
    "[": "111/100/100/100/100/100/111", "]": "111/001/001/001/001/001/111",
    "{": "011/010/010/100/010/010/011", "}": "110/010/010/001/010/010/110",
    "+": "00000/00100/00100/11111/00100/00100/00000",
    "=": "00000/00000/11111/00000/11111/00000/00000",
    "<": "0001/0010/0100/1000/0100/0010/0001", ">": "1000/0100/0010/0001/0010/0100/1000",
    "*": "00000/10101/01110/11111/01110/10101/00000",
    "#": "01010/01010/11111/01010/11111/01010/01010",
    "%": "11001/11001/00010/00100/01000/10011/10011",
    "&": "01100/10010/10100/01000/10101/10010/01101",
    "@": "01110/10001/10111/10101/10111/10000/01110",
    "$": "00100/01111/10100/01110/00101/11110/00100",
    "^": "00100/01010/10001/00000/00000/00000/00000",
    "`": "10/01/00/00/00/00/00", "|": "1/1/1/1/1/1/1",
    "~": "00000/00000/01001/10110/00000/00000/00000",
    "·": "0/0/0/1/0/0/0", "…": "00000/00000/00000/00000/00000/00000/10101",
    "–": "00000/00000/00000/11111/00000/00000/00000",
    "—": "0000000/0000000/0000000/1111111/0000000/0000000/0000000",
}
for char, source in {"\u00a0": " ", "‘": "'", "’": "'", "“": '"', "”": '"'}.items():
    PATTERNS[char] = PATTERNS[source]

root = Path(__file__).resolve().parents[1]
cmap = {ord(c): f"uni{ord(c):04X}" for c in PATTERNS}
KERNING_CANDIDATES = (
    "AV", "AW", "AY", "VA", "Ve", "Vo", "Va", "WA", "YA", "LY", "LV",
    "Ty", "Tr", "Tw", "Tu", "Fa", "Fe", "Fr", "Fy", "Po", "Pe",
    "Wo", "We", "Wi", "Ya", "Ye", "Yu", "At", "Aw", "Ay", "Ta",
    "Te", "To", "Yo", "Wa", "Fo", "Pa", "LT", "FA",
    "it", "il", "li", "ft", "rt", "fi", "fl", "r.", "f.", "T.",
    "V.", "W.", "Y.",
)


def weighted_rows(source, added_columns):
    """Add rightward pixel columns to each run, preserving open one-pixel gaps."""
    rows = source.split("/")
    output = []
    for pixels in rows:
        runs = []
        i = 0
        while i < len(pixels):
            if pixels[i] != "1":
                i += 1
                continue
            start = i
            end = i + 1
            while end < len(pixels) and pixels[end] == "1":
                end += 1
            runs.append((start, end))
            i = end
        result = []
        cursor = 0
        for index, (start, end) in enumerate(runs):
            result.append("0" * (start - cursor))
            result.append("1" * (end - start))
            gap = runs[index + 1][0] - end if index + 1 < len(runs) else added_columns
            # Keep at least one clear pixel between separate stems/counters.
            extension = min(added_columns, max(0, gap - 1)) if index + 1 < len(runs) else added_columns
            result.append("1" * extension)
            cursor = end + extension
        result.append("0" * (len(pixels) - cursor))
        output.append("".join(result))
    width = max(map(len, output))
    return [row.ljust(width, "0") for row in output]


def duplicate_columns(source, columns):
    """Thicken selected stems while keeping every blank channel as a blank."""
    rows = source.split("/")
    width = len(rows[0])
    selected = {column % width for column in columns}
    output = []
    for row in rows:
        result = []
        for column, pixel in enumerate(row):
            result.append(pixel)
            if column in selected:
                result.append(pixel)
        output.append("".join(result))
    max_width = max(map(len, output))
    return [row.ljust(max_width, "0") for row in output]


def bitmap(char, weight):
    source = PATTERNS.get(char, "11111/10001/10101/10101/10101/10001/11111")
    added_columns = {400: 0, 600: 1, 700: 2}[weight]
    # At the 8px target, extending every horizontal run makes small letters
    # such as a/m/y read as solid blobs. Duplicate the outer vertical stems
    # instead: the interior channels keep their original clear pixels, while
    # counters remain the same size. Apply the same treatment to any glyph
    # with enclosed counters, including digits and punctuation.
    protected = (char in "amyRMA" if char is not None else False) or \
        enclosed_pixel_regions(source.split("/")) > 0
    if not added_columns:
        rows = source.split("/")
    elif protected:
        edge_columns = [0] if weight == 600 else [0, -1]
        rows = duplicate_columns(source, edge_columns)
    else:
        rows = weighted_rows(source, added_columns)
    # Keep spaces and blank glyph rows at their original advance width.
    return rows


def pixel_set(rows):
    return {(col * 100, (6 - row) * 100) for row, pixels in enumerate(rows)
            for col, pixel in enumerate(pixels) if pixel == "1"}


def enclosed_pixel_regions(rows):
    """Count enclosed blank pixel regions, using a one-cell exterior border."""
    height, width = len(rows), len(rows[0])
    blocked = {(x + 1, y + 1) for y, row in enumerate(rows)
               for x, pixel in enumerate(row) if pixel == "1"}
    seen = set()
    components = 0
    for y in range(height + 2):
        for x in range(width + 2):
            if (x, y) in blocked or (x, y) in seen:
                continue
            stack = [(x, y)]
            seen.add((x, y))
            exterior = False
            while stack:
                cx, cy = stack.pop()
                if cx in (0, width + 1) or cy in (0, height + 1):
                    exterior = True
                for point in ((cx - 1, cy), (cx + 1, cy), (cx, cy - 1), (cx, cy + 1)):
                    px, py = point
                    if (0 <= px < width + 2 and 0 <= py < height + 2 and
                            point not in blocked and point not in seen):
                        seen.add(point)
                        stack.append(point)
            if not exterior:
                components += 1
    return components


def pair_gap(left_pixels, right_pixels, left_advance, adjustment):
    right_origin = left_advance + adjustment
    deltas = [right_origin + rx - lx for lx, ly in left_pixels
              for rx, ry in right_pixels if ly == ry]
    # Pixel rectangles are half-open 100-unit cells; touching leaves no gap.
    if any(delta < 100 for delta in deltas):
        return -1
    gaps = [delta - 100 for delta in deltas]
    return min(gaps) if gaps else None


def build_font(weight, style, filename):
    font = FontBuilder(800, isTTF=True)
    font.setupGlyphOrder([".notdef", *cmap.values()])
    font.setupCharacterMap(cmap)
    glyphs, metrics, pixels_by_char = {}, {}, {}
    digit_advance = (max(len(row) for digit in "0123456789"
                        for row in bitmap(digit, weight)) + 1) * 100
    for char, name in [(None, ".notdef"), *[(c, cmap[ord(c)]) for c in PATTERNS]]:
        rows = bitmap(char, weight)
        assert len({len(row) for row in rows}) == 1
        pixels_by_char[char] = pixel_set(rows)
        pen = TTGlyphPen(None)
        for row, pixels in enumerate(rows):
            # Join adjacent pixels into runs so there are no internal seams.
            start = None
            for col, pixel in enumerate(pixels + "0"):
                if pixel == "1" and start is None:
                    start = col
                elif pixel == "0" and start is not None:
                    x, right, y = start * 100, col * 100, (6 - row) * 100
                    pen.moveTo((x, y)); pen.lineTo((x, y + 100))
                    pen.lineTo((right, y + 100)); pen.lineTo((right, y)); pen.closePath()
                    start = None
        glyph = pen.glyph()
        if glyph.numberOfContours:
            assembly = ["RTG[ ]"]
            for axis in (1, 0):
                assembly.append(f"SVTCA[{axis}]")
                for point in range(len(glyph.coordinates)):
                    assembly.extend(["PUSHW[ ]", str(point), "MDAP[1]"])
            glyph.program = Program(); glyph.program.fromAssembly(assembly)
        glyphs[name] = glyph
        advance = digit_advance if char is not None and char in "0123456789" else (len(rows[0]) + 1) * 100
        metrics[name] = (advance, 0)
    font.setupGlyf(glyphs)
    font.setupHorizontalMetrics(metrics)
    font.setupHorizontalHeader(ascent=700, descent=-100)
    weight_class = weight
    fs_selection = 0xC0 if weight == 400 else (0x80 if weight == 600 else 0xA0)
    font.setupOS2(version=4, sTypoAscender=700, sTypoDescender=-100, sTypoLineGap=0,
                  usWinAscent=700, usWinDescent=100, usWeightClass=weight_class,
                  fsSelection=fs_selection)
    ps_style = style.replace(" ", "")
    font.setupNameTable({"familyName": "RBX Cartridge", "styleName": style,
        "uniqueFontIdentifier": f"RBXCartridge1.0-{ps_style}",
        "fullName": f"RBX Cartridge {style}" if weight != 400 else "RBX Cartridge",
        "psName": f"RBXCartridge-{ps_style}", "version": "Version 1.0"})
    font.setupPost()
    font.font["head"].macStyle = 0x1 if weight == 700 else 0
    font.font["maxp"].maxZones = 1
    font.font["maxp"].maxStackElements = 1
    font.font["maxp"].maxSizeOfInstructions = max(
        len(g.program.getBytecode()) for g in glyphs.values() if hasattr(g, "program"))
    font.font["gasp"] = newTable("gasp"); font.font["gasp"].gaspRange = {65535: 1}

    pairs = []
    for pair in KERNING_CANDIDATES:
        left, right = pair
        left_advance = metrics[cmap[ord(left)]][0]
        if pair_gap(pixels_by_char[left], pixels_by_char[right], left_advance, -100) >= 100:
            pairs.append((left, right))
    if pairs:
        feature = "languagesystem DFLT dflt;\nlanguagesystem latn dflt;\nfeature kern {\n"
        feature += "".join(
            f"  pos {cmap[ord(left)]} {cmap[ord(right)]} -100;\n"
            for left, right in pairs)
        feature += "} kern;\n"
        addOpenTypeFeaturesFromString(font.font, feature)
    font.font.flavor = "woff2"
    output = root / "assets/fonts" / filename
    font.save(output)
    return output, pairs, pixels_by_char, metrics


builds = [
    (400, "Regular", "rbx-cartridge.woff2"),
    (600, "Semibold", "rbx-cartridge-semibold.woff2"),
    (700, "Bold", "rbx-cartridge-bold.woff2"),
]
results = [build_font(*args) for args in builds]
coverage = None
for (weight, style, filename), (output, pairs, pixels, metrics) in zip(builds, results):
    check = TTFont(output)
    current_coverage = set(check.getBestCmap())
    assert all(c in current_coverage for c in range(32, 127))
    assert coverage is None or current_coverage == coverage
    coverage = current_coverage
    assert check["OS/2"].usWeightClass == weight
    assert len({check["hmtx"][cmap[ord(c)]][0] for c in "0123456789"}) == 1
    if weight == 600:
        assert check["OS/2"].fsSelection & 0x40 == 0
    if weight == 700:
        assert check["head"].macStyle & 0x1
    for name in check.getGlyphOrder():
        glyph = check["glyf"][name]
        if glyph.numberOfContours:
            assert all(x % 100 == 0 and y % 100 == 0 for x, y in glyph.coordinates)
    for left, right in pairs:
        gap = pair_gap(pixels[left], pixels[right], metrics[cmap[ord(left)]][0], -100)
        assert gap is None or gap >= 100
    print(f"{style} ({weight}): {output.name}; kerning: " +
          (", ".join(left + right for left, right in pairs) or "none"))
    assert results[0][2] != results[1][2] and results[1][2] != results[2][2]
for char, source in PATTERNS.items():
    original_holes = enclosed_pixel_regions(source.split("/"))
    if original_holes:
        assert enclosed_pixel_regions(bitmap(char, 600)) >= original_holes
        assert enclosed_pixel_regions(bitmap(char, 700)) >= original_holes
print(f"RBX Cartridge: {len(cmap)} glyphs; shared coverage, tabular digits, whole-pixel grid verified")
