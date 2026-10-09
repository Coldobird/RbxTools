"""Generate RBX Micro: a native 3x5-pixel alphabet for six-pixel footers."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

# Original tiny glyph designs; lowercase uses the compact uppercase alphabet.
PATTERNS = {
    "A": "010/101/111/101/101", "B": "110/101/110/101/110",
    "C": "011/100/100/100/011", "D": "110/101/101/101/110",
    "E": "111/100/110/100/111", "F": "111/100/110/100/100",
    "G": "011/100/101/101/011", "H": "101/101/111/101/101",
    "I": "111/010/010/010/111", "J": "001/001/001/101/010",
    "K": "101/101/110/101/101", "L": "100/100/100/100/111",
    "M": "101/111/111/101/101", "N": "101/111/111/111/101",
    "O": "010/101/101/101/010", "P": "110/101/110/100/100",
    "Q": "010/101/101/111/011", "R": "110/101/110/101/101",
    "S": "011/100/010/001/110", "T": "111/010/010/010/010",
    "U": "101/101/101/101/111", "V": "101/101/101/101/010",
    "W": "101/101/111/111/101", "X": "101/101/010/101/101",
    "Y": "101/101/010/010/010", "Z": "111/001/010/100/111",
    "0": "111/101/101/101/111", "1": "010/110/010/010/111",
    "2": "110/001/010/100/111", "3": "110/001/010/001/110",
    "4": "101/101/111/001/001", "5": "111/100/110/001/110",
    "6": "011/100/111/101/111", "7": "111/001/010/010/010",
    "8": "111/101/111/101/111", "9": "111/101/111/001/110",
    ".": "0/0/0/0/1", "-": "000/000/111/000/000",
    " ": "00/00/00/00/00",
}
root = Path(__file__).resolve().parents[1]
font = FontBuilder(600, isTTF=True)
order = [".notdef"] + [f"uni{ord(c):04X}" for c in PATTERNS]
font.setupGlyphOrder(order)
cmap = {ord(c): f"uni{ord(c):04X}" for c in PATTERNS}
cmap.update({ord(c.lower()): name for c, name in [(c, cmap[ord(c)]) for c in PATTERNS if c.isalpha()]})
font.setupCharacterMap(cmap)
glyphs, metrics = {}, {}
for char, name in [(None, ".notdef")] + [(c, cmap[ord(c)]) for c in PATTERNS]:
    pen = TTGlyphPen(None)
    rows = PATTERNS.get(char, "111/101/101/101/111").split("/")
    for row, pixels in enumerate(rows):
        for col, pixel in enumerate(pixels):
            if pixel == "1":
                x, y = col * 100, (4 - row) * 100
                pen.moveTo((x, y)); pen.lineTo((x, y + 100))
                pen.lineTo((x + 100, y + 100)); pen.lineTo((x + 100, y)); pen.closePath()
    glyphs[name] = pen.glyph()
    metrics[name] = ((len(rows[0]) + 1) * 100, 0)
font.setupGlyf(glyphs)
font.setupHorizontalMetrics(metrics)
font.setupHorizontalHeader(ascent=500, descent=-100)
font.setupOS2(sTypoAscender=500, sTypoDescender=-100, usWinAscent=500, usWinDescent=100)
font.setupNameTable({"familyName": "RBX Micro", "styleName": "Regular", "uniqueFontIdentifier": "RBXMicro1.0",
                     "fullName": "RBX Micro", "psName": "RBXMicro-Regular", "version": "Version 1.0"})
font.setupPost()
font.font.flavor = "woff2"
font.save(root / "assets/fonts/rbx-micro.woff2")
print("RBX Micro: native 3x5 pixel glyphs at 6px; alphabet, digits, version punctuation")
