"""Draw the icon plates the game does not ship.

The overlay's controls sit on the game's own 21x21 button plates. Two of them
exist — Button_Close and Button_Minimize — and two do not: there is no dashboard
mark in Hero Siege's UI atlas and no reset mark either.

They are not drawn from nothing. Diffing the sprites shows exactly how the game
builds them: close.png and close_hover.png are pixel-identical inside a 13x13
well at (4, 4) and differ only in the 162 pixels of frame around it, and
minimize.png is close.png with that well repainted. So a plate is one frame, one
hover frame, and a glyph stamped into both — the hover state lights the rim and
leaves the mark alone, which is the convention this follows.

The palette is the sheet's own: #7f1d1c for the mark, #2d0b0b a pixel beneath it
for the drop shadow, black behind. Nothing here invents a colour, so a season's
recolour in gen_skin.py treats these like any other plate.

    python tools/gen_plates.py            # writes src/assets/game/
    python tools/gen_plates.py --preview  # also a sheet at 8x to look at
"""

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).parent.parent
ART = ROOT / "src" / "assets" / "game"

WELL = (4, 4, 13, 13)  # x, y, w, h — where a mark goes
BODY = (0x7F, 0x1D, 0x1C, 255)
SHADE = (0x2D, 0x0B, 0x0B, 255)
GROUND = (0, 0, 0, 255)

# 13x13, one character per pixel. '#' is the mark; the shadow is worked out from
# it rather than drawn, so a glyph cannot be edited into an inconsistent one.
GLYPHS = {
    # Three rising bars: the dashboard is where the numbers are.
    "dashboard": [
        ".............",
        ".............",
        ".............",
        ".........###.",
        ".........###.",
        ".....###.###.",
        ".....###.###.",
        ".###.###.###.",
        ".###.###.###.",
        ".###.###.###.",
        ".............",
        ".............",
        ".............",
    ],
    # The one the game ships is broken and has been on screen all along: its
    # hover frame lost the rim entirely and lit the glyph gold instead, so
    # hovering it makes the plate vanish and leaves a dash floating. Redrawing it
    # from the shipped rest frame — a 10x1 bar with the usual drop — reproduces
    # minimize.png byte for byte and gives it a hover frame that behaves.
    "minimize": [
        ".............",
        ".............",
        ".............",
        ".............",
        ".............",
        ".##########..",
        ".............",
        ".............",
        ".............",
        ".............",
        ".............",
        ".............",
        ".............",
    ],
    # Skip-to-start. A circular arrow is the usual mark for this and it turns to
    # mush at thirteen pixels; back-to-the-beginning survives the size and says
    # the same thing.
    "reset": [
        ".............",
        ".............",
        ".............",
        ".............",
        "..##.....#...",
        "..##....##...",
        "..##...###...",
        "..##..####...",
        "..##...###...",
        "..##....##...",
        "..##.....#...",
        ".............",
        ".............",
    ],
}


def stamp(frame: Image.Image, glyph: list[str]) -> Image.Image:
    """One plate: the frame as it is, with the well repainted."""
    out = frame.copy()
    px = out.load()
    x0, y0, w, h = WELL
    mark = {(x, y) for y, row in enumerate(glyph) for x, c in enumerate(row) if c == "#"}
    # a pixel under the mark, where the mark itself is not — the same one-pixel
    # drop the shipped minimize plate uses
    shadow = {(x, y + 1) for (x, y) in mark} - mark

    for y in range(h):
        for x in range(w):
            at = (x0 + x, y0 + y)
            if (x, y) in mark:
                px[at] = BODY
            elif (x, y) in shadow:
                px[at] = SHADE
            else:
                px[at] = GROUND
    return out


def main() -> None:
    rest = Image.open(ART / "close.png").convert("RGBA")
    hover = Image.open(ART / "close_hover.png").convert("RGBA")
    if rest.size != (21, 21):
        raise SystemExit(f"close.png is {rest.size}, expected (21, 21)")

    made = []
    for name, glyph in GLYPHS.items():
        if len(glyph) != 13 or any(len(r) != 13 for r in glyph):
            raise SystemExit(f"{name}: a glyph is 13x13")
        stamp(rest, glyph).save(ART / f"{name}.png")
        stamp(hover, glyph).save(ART / f"{name}_hover.png")
        made += [f"{name}.png", f"{name}_hover.png"]
    print(f"{len(made)} plates -> {ART}")
    print("  run tools/gen_skin.py afterwards: a season needs its own copies")

    if "--preview" in sys.argv:
        names = ["close.png", "minimize.png"] + made
        scale, pad = 8, 6
        sheet = Image.new("RGBA", (len(names) * (21 * scale + pad) + pad, 21 * scale + 2 * pad), (24, 16, 16, 255))
        for i, n in enumerate(names):
            tile = Image.open(ART / n).convert("RGBA").resize((21 * scale, 21 * scale), Image.NEAREST)
            sheet.alpha_composite(tile, (pad + i * (21 * scale + pad), pad))
        out = ROOT / "tools" / "data" / "plates.png"
        out.parent.mkdir(parents=True, exist_ok=True)
        sheet.convert("RGB").save(out)
        print(f"  preview -> {out}")


if __name__ == "__main__":
    main()
