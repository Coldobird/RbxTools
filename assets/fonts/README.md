# RBX Cartridge (active UI font)

An original pixel alphabet drawn as explicit grids in
`scripts/build-cartridge-font.py`. It reads no existing font and does not reuse
Press Start 2P outlines, spacing tables, or hint programs. Capitals are seven
pixels high, lowercase has a five-pixel x-height, and narrow letters have their
own widths. A one-pixel gap separates letters before optical pair adjustments;
digits keep equal widths.

Regular (400), Semibold (600), and Bold (700) are separate font files with
distinct pixel outlines. The heavier weights expand stems while preserving
counter spaces. Pair spacing is stored in the GPOS kern feature and enabled
with `font-kerning: normal`. CSS applies bold to the brand, page/dialog titles,
and semibold to card titles. Weights 600/700 are reserved for roles at 16px
and above: 8px actions, navigation, setting labels, and body copy use regular
to keep their small counters legible. The compact 8px brand also uses regular.
Headings and controls use zero
extra tracking so their font-level pair spacing remains effective.

The font contains 104 glyphs covering printable ASCII, smart quotes, dashes,
ellipsis, and middle dot. Unmodified Press Start 2P is retained as a fallback
for other scripts and accented characters. The small sidebar footer uses the
original RBX Micro alphabet described below.

Rebuild with `python scripts/build-cartridge-font.py` (requires fonttools and
brotli). Its validation checks printable ASCII coverage, digit widths, and
pixel-grid coordinates. The checked-in WOFF2 is used directly by normal builds.
Use 8, 16, or 24px to preserve the native grid at 100% zoom.

## Earlier RBX Pixel experiment (not used by the UI)

A locally modified, proportional-spacing derivative of Press Start 2P, licensed
under SIL OFL 1.1 (see OFL.txt). The derivative uses a new family and PostScript
name to respect the original font's Reserved Font Name.

The original square-pixel outlines are preserved. Non-mark glyphs are shifted
to remove surplus left bearings and given one design pixel of trailing space.
Narrow punctuation no longer occupies a full letter cell. Spaces use half an
em; digits retain the original tabular widths. Selected Latin pairs receive
one design pixel of kerning only where their row outlines retain at least one
pixel of separation. All five original language subsets remain bundled.

Point order and composite structure are preserved when translating glyphs.
Simple glyphs use direct grid-snapping hints rather than optical autohints.
The gasp table requests grid fitting without
grayscale smoothing through 24 ppem. UI sizes follow the original 8-pixel em
(8, 16, 24 CSS pixels) and text avoids fractional vertical transforms. This
targets crisp rendering at 100% zoom; browser/OS scaling can resample pixels.

Transparent text wrappers also threshold their alpha through an SVG filter,
because Chromium may smooth font edges regardless of the gasp request. This
keeps the font's color and makes its edge pixels fully on or off.

RBX Micro is a separate, original 3x5-pixel alphabet used at 6px for the sidebar
version. Rebuild it with `python scripts/build-micro-font.py`. Its glyph source
is in the generator; it is not a scaled-down derivative of the larger font.

Regenerate with `python scripts/build-pixel-font.py` after `npm ci` and
`python -m pip install fonttools==4.66.1 brotli==1.2.0`. Generated WOFF2 files are
checked in, so normal application builds do not require Python. The script
also copies the original license to public/fonts/OFL.txt for distribution.
