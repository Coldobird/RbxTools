"""Additional pixel-native optical masters. Existing font files are untouched.

10px has a separately drawn 9-row alphabet. 18/26px retain uniform 2/3px
strokes from the original grid, with two extra rows along letter stems.
They intentionally avoid fractional enlargement of the original outlines.
"""
import importlib.util
import sys
from pathlib import Path
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("cartridge", Path(__file__).with_name("build-cartridge-font.py"))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)

# Explicit nine-row capitals and six-row lowercase bodies, with ascenders
# above the x-height and descenders below the baseline. Blank channels are
# part of the design, especially the twin shoulders in m and the bowl in R.
CAPS = {
    'A':'001100/010010/100001/100001/100001/111111/100001/100001/100001',
    'B':'111110/100001/100001/100001/111110/100001/100001/100001/111110',
    'C':'011111/100000/100000/100000/100000/100000/100000/100000/011111',
    'D':'111100/100010/100001/100001/100001/100001/100001/100010/111100',
    'E':'11111/10000/10000/10000/11110/10000/10000/10000/11111',
    'F':'11111/10000/10000/10000/11110/10000/10000/10000/10000',
    'G':'011110/100001/100000/100000/100111/100001/100001/100001/011110',
    'H':'100001/100001/100001/100001/111111/100001/100001/100001/100001',
    'I':'111/010/010/010/010/010/010/010/111',
    'J':'00111/00010/00010/00010/00010/00010/10010/10010/01100',
    'K':'100001/100010/100100/101000/110000/101000/100100/100010/100001',
    'L':'10000/10000/10000/10000/10000/10000/10000/10000/11111',
    'M':'1000001/1100011/1010101/1001001/1001001/1000001/1000001/1000001/1000001',
    'N':'100001/110001/110001/101001/101001/100101/100101/100011/100001',
    'O':'011110/100001/100001/100001/100001/100001/100001/100001/011110',
    'P':'111110/100001/100001/100001/111110/100000/100000/100000/100000',
    'Q':'011110/100001/100001/100001/100001/100001/100101/100010/011101',
    'R':'111110/100001/100001/100001/111110/101000/100100/100010/100001',
    'S':'011111/100000/100000/100000/011110/000001/000001/000001/111110',
    'T':'11111/00100/00100/00100/00100/00100/00100/00100/00100',
    'U':'100001/100001/100001/100001/100001/100001/100001/100001/011110',
    'V':'100001/100001/100001/100001/100001/010010/010010/001100/001100',
    'W':'1000001/1000001/1000001/1000001/1001001/1001001/1010101/1100011/1000001',
    'X':'100001/100001/010010/010010/001100/010010/010010/100001/100001',
    'Y':'10001/10001/01010/01010/00100/00100/00100/00100/00100',
    'Z':'111111/000001/000010/000010/000100/001000/001000/010000/111111',
    '0':'011110/100001/100011/100101/100101/101001/110001/100001/011110',
    '1':'0010/0110/1010/0010/0010/0010/0010/0010/1111',
    '2':'011110/100001/000001/000001/000010/000100/001000/010000/111111',
    '3':'111110/000001/000001/000001/001110/000001/000001/000001/111110',
    '4':'000010/000110/001010/010010/100010/111111/000010/000010/000010',
    '5':'111111/100000/100000/100000/111110/000001/000001/000001/111110',
    '6':'011110/100000/100000/100000/111110/100001/100001/100001/011110',
    '7':'111111/000001/000010/000010/000100/000100/001000/001000/001000',
    '8':'011110/100001/100001/100001/011110/100001/100001/100001/011110',
    '9':'011110/100001/100001/100001/011111/000001/000001/000001/011110',
}
LOWER = {
    'a':'011110/000001/011111/100001/100001/011111',
    'b':'111110/100001/100001/100001/100001/111110',
    'c':'01111/10000/10000/10000/10000/01111',
    'd':'011111/100001/100001/100001/100001/011111',
    'e':'011110/100001/111111/100000/100000/011111',
    'f':'1111/0100/0100/0100/0100/0100',
    'g':'011111/100001/100001/011111/000001/000001/111110',
    'h':'111110/100001/100001/100001/100001/100001',
    'i':'11/01/01/01/01/01',
    'j':'011/001/001/001/001/001/110',
    'k':'10001/10010/10100/11000/10100/10011',
    'l':'01/01/01/01/01/01',
    'm':'1110110/1001001/1001001/1001001/1001001/1001001',
    'n':'111110/100001/100001/100001/100001/100001',
    'o':'011110/100001/100001/100001/100001/011110',
    'p':'111110/100001/100001/100001/111110/100000/100000',
    'q':'011111/100001/100001/100001/011111/000001/000001',
    'r':'10111/11000/10000/10000/10000/10000',
    's':'01111/10000/01110/00001/00001/11110',
    't':'1111/0100/0100/0100/0100/0011',
    'u':'100001/100001/100001/100001/100001/011111',
    'v':'100001/100001/100001/010010/010010/001100',
    'w':'1000001/1000001/1001001/1001001/1010101/0100010',
    'x':'10001/01010/00100/00100/01010/10001',
    'y':'100001/100001/100001/100001/011111/000001/111110',
    'z':'11111/00001/00010/00100/01000/11111',
}


def small_rows(char):
    char = char or '\ufffd'
    if char in CAPS:
        return CAPS[char].split('/')
    if char in LOWER:
        rows = LOWER[char].split('/')
        width = len(rows[0])
        blank = '0' * width
        top = [blank] * 3
        if char in 'bhk':
            top = ['1' + '0' * (width - 1)] * 3
        elif char == 'd':
            top = ['0' * (width - 1) + '1'] * 3
        elif char == 'l':
            top = ['11', '01', '01']
        elif char == 'f':
            top = ['0011', '0100', '0100']
        elif char == 't':
            top = ['0000', '0100', '0100']
        elif char in 'ij':
            top = ['0' * (width - 1) + '1', blank, blank]
        return top + rows
    rows = base.PATTERNS.get(char, '11111/10001/10101/10101/10101/10001/11111').split('/')
    # Top punctuation keeps its original height; baseline punctuation is
    # lowered two rows. Full-height symbols gain two central stem rows.
    if char in "'\"`‘’“”":
        return rows + ['0' * len(rows[0])] * 2
    if char in '()[]{}|/\\!?':
        return rows[:3] + [rows[3]] * 2 + rows[3:]
    return ['0' * len(rows[0])] * 2 + rows


def rows_at_size(char, weight, size):
    if size == 10:
        rows = small_rows(char)
    else:
        scale = {18: 2, 26: 3}[size]
        original = base.bitmap(char, 400)
        # Elongate stem rows, rather than resampling every stroke. Punctuation
        # keeps its stroke dimensions and gains two pixels of leading.
        extra = {3: 2} if char and char.islower() else {1: 1, 5: 1}
        letter = char is not None and char.isalnum()
        rows = [] if letter else ['0' * (len(original[0]) * scale)] * 2
        for index, row in enumerate(original):
            rows += [''.join(pixel * scale for pixel in row)] * (scale + (extra.get(index, 0) if letter else 0))
    if weight != 400 and any('1' in row for row in rows):
        # Expand stems by duplicating columns, never consume counter space.
        columns = [0] if weight == 600 else [0, -1]
        rows = base.duplicate_columns('/'.join(rows), columns)
    return rows


def main():
    for size in (10, 18, 26):
        prior = None
        for weight, style, suffix in ((400, 'Regular', ''), (600, 'Semibold', '-semibold'), (700, 'Bold', '-bold')):
            raster = lambda char, w: rows_at_size(char, w, size)
            result = base.build_font(weight, style, f'rbx-cartridge-{size}{suffix}.woff2',
                pixel_em=size, family=f'RBX Cartridge {size}', rows_for_glyph=raster,
                spacing_pixels=1 if size == 10 else size // 8,
                descent_pixels=1 if size == 10 else size // 8)
            output, pairs, pixels, metrics = result
            font = TTFont(output)
            assert font['head'].unitsPerEm == size * 100
            assert set(font.getBestCmap()) == set(base.cmap)
            assert font['OS/2'].usWeightClass == weight
            assert all(a % 100 == 0 and b % 100 == 0 for a, b in metrics.values())
            for glyph in font['glyf'].glyphs.values():
                glyph.expand(font['glyf'])
                if glyph.numberOfContours:
                    assert all(x % 100 == 0 and y % 100 == 0 for x, y in glyph.coordinates)
            for char in base.PATTERNS:
                assert base.enclosed_pixel_regions(raster(char, weight)) >= base.enclosed_pixel_regions(raster(char, 400))
            assert prior is None or pixels != prior
            prior = pixels
            print(f'{output.name}: {len(base.cmap)} glyphs, {len(pairs)} kerning pairs, native {size}px grid verified')


if __name__ == '__main__':
    main()
