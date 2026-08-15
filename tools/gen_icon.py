"""Draw the app icon and everything Windows and the tray want made of it.

The mark is designed on a 16x16 grid — the size it has to survive in a taskbar —
and every larger size is that same grid with bigger squares. Nothing is ever
resampled smoothly: the game is pixel art, and an icon that softens its edges
when it grows stops looking like it belongs to the game.

The coins are the game's own sprite, one frame of `coin_strip.png`, so the gold
is exactly the gold the player sees on screen.

    python tools/gen_icon.py            # writes src-tauri/icons/*
    python tools/gen_icon.py --preview  # also writes a contact sheet to look at
    python tools/gen_icon.py --discord  # artwork for the Discord application:
                                        # logo, the satanic badge, invite cover

Run tools/gen_installer_art.py afterwards: the installer's header and sidebar
are drawn from the icon.
"""

import math
import random
import struct
import sys
from io import BytesIO
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).parent.parent
ART = ROOT / "src" / "assets" / "game"
ICONS = ROOT / "src-tauri" / "icons"

# straight out of the game's panel, button and coin sprites
BLACK = (18, 9, 11, 255)
PLATE = (34, 21, 23, 255)
CRIMSON = (150, 37, 56, 255)
BONE = (232, 216, 168, 255)
NONE = (0, 0, 0, 0)

GRID = 16
# every size the .ico carries, plus the standalone files
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

# HS as strokes on the grid: two posts and a waist, then three bars and two
# half-posts. Curves are what make small letters mush, so there are none.
LETTERS = [
    (0, 0, 1, 8),
    (4, 0, 5, 8),
    (2, 3, 3, 5),
    (7, 0, 12, 1),
    (7, 2, 8, 3),
    (7, 4, 12, 5),
    (11, 6, 12, 7),
    (7, 7, 12, 8),
]
LETTERS_AT = (2, 2)
# The pile, back to front. It sits in front of the plate rather than inside it,
# which is why the coins overlap the bottom rim — gold heaped against a sign.
COINS = [(1, 11, 6), (6, 12, 6), (11, 11, 5)]


def coin(size: int) -> Image.Image:
    """One frame of the coin animation, at the size the pile needs."""
    strip = Image.open(ART / "coin_strip.png").convert("RGBA")
    frame = strip.crop((0, 0, strip.height, strip.height))
    return frame.resize((size, size), Image.NEAREST)


def mark() -> Image.Image:
    """The icon at its true resolution: 16 by 16."""
    img = Image.new("RGBA", (GRID, GRID), NONE)
    d = ImageDraw.Draw(img)

    # the game's plate: dark slab, crimson rim, corners cut off
    d.rectangle([0, 0, GRID - 1, GRID - 1], fill=PLATE, outline=CRIMSON)
    d.rectangle([1, 1, GRID - 2, GRID - 2], outline=BLACK)
    d.rectangle([2, 2, GRID - 3, GRID - 3], fill=PLATE)
    for x, y in [(0, 0), (GRID - 1, 0), (0, GRID - 1), (GRID - 1, GRID - 1)]:
        d.point((x, y), fill=NONE)

    # the letters, with a hard shadow under them so they hold against the gold
    ox, oy = LETTERS_AT
    for dx, dy in [(1, 1), (0, 0)]:
        colour = BLACK if (dx, dy) == (1, 1) else BONE
        for x0, y0, x1, y1 in LETTERS:
            d.rectangle([ox + x0 + dx, oy + y0 + dy, ox + x1 + dx, oy + y1 + dy], fill=colour)

    # The letters are wider than the plate's inside, so their shadow lands on the
    # rim and eats it. Draw the rim again over them: the frame stays whole and the
    # letters read as sitting on the plate, which is what they do in the game.
    d.rectangle([0, 0, GRID - 1, GRID - 1], outline=CRIMSON)
    for x, y in [(0, 0), (GRID - 1, 0), (0, GRID - 1), (GRID - 1, GRID - 1)]:
        d.point((x, y), fill=NONE)

    # last, so the pile heaps against the sign rather than sitting behind it
    for x, y, size in COINS:
        img.alpha_composite(coin(size), (x, y))
    return img


def at(size: int, base: Image.Image) -> Image.Image:
    """The mark at a size, always as whole squares."""
    return base.resize((size, size), Image.NEAREST)


GOLD = (195, 175, 118, 255)
DIM = (140, 118, 104, 255)

def heap(width: int, floor: int) -> list[tuple[int, int, int]]:
    """A pile of coins along the bottom edge, as (x, y, size), back row first.

    Evenly spaced coins read as a stripe of sweets. A heap is coins overlapping
    coins, each one sunk a little into the edge, with nothing at a spacing the
    eye can count — hence the jitter, from a fixed seed so the picture is the
    same every time it is drawn."""
    rng = random.Random(7)
    # gold does not lie level: the pile crests twice across the picture and
    # sinks between, which is what stops the band looking poured
    swell = lambda x: 0.5 + 0.5 * math.cos((x / width) * math.tau * 1.5 + 0.9)
    pile = []
    # back to front: the tall row stands up, the near rows bank in front of it
    for step, low, high in [(34, 42, 102), (30, 28, 66), (26, 12, 38)]:
        x = -30
        while x < width + 30:
            shown = int(low + (high - low) * swell(x)) + rng.randint(-6, 6)
            size = shown + rng.randint(8, 20)
            pile.append((x, floor - shown, size))
            x += step + rng.randint(-6, 8)
    return pile


def wash(size: tuple[int, int], colour: tuple[int, int, int], alpha) -> Image.Image:
    """A top-to-bottom veil of one colour, its strength given per row as a
    fraction of the height. It has to be its own layer: drawing a colour that
    carries alpha writes that alpha into the picture instead of blending it."""
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    w, h = size
    for y in range(h):
        a = max(0, min(255, alpha(y / h)))
        if a:
            d.line([(0, y), (w, y)], fill=colour + (a,))
    return layer


def cover(base: Image.Image) -> Image.Image:
    """The 16:9 illustration Discord puts on an invitation, and the banner the
    README opens with — the same picture serves both, so the project has one
    face rather than two.

    A wide picture is not the icon stretched: the plate becomes the frame around
    everything, the mark keeps its square inside it, the name stands beside it,
    and the gold heaps along the bottom the way it heaps against the plate."""
    w, h = 1024, 576
    img = Image.new("RGBA", (w, h), PLATE)
    d = ImageDraw.Draw(img)

    # the game's own panels are lit from the top, and a dark floor is what the
    # gold has to glow against
    img.alpha_composite(wash((w, h), BLACK[:3], lambda t: int(80 * t**2)))
    # what the pile throws back up the picture: without it the gold sits on the
    # dark like a sticker, with it the bottom of the frame is lit by it
    img.alpha_composite(wash((w, h), (208, 104, 34), lambda t: int(46 * max(0.0, t - 0.6) ** 2 / 0.16)))

    # the plate's rim, at the icon's own proportions
    d.rectangle([0, 0, w - 1, h - 1], outline=CRIMSON, width=14)
    d.rectangle([14, 14, w - 15, h - 15], outline=BLACK, width=8)

    # the plate stands in the gold rather than floating over it
    img.alpha_composite(at(320, base), (86, 150))

    font = ROOT / "src" / "assets" / "fonts" / "cookierunbold.ttf"
    title = ImageFont.truetype(str(font), 84)
    line = ImageFont.truetype(str(font), 27)
    for dx, dy, fill in [(5, 6, BLACK), (0, 0, GOLD)]:
        d.text((462 + dx, 236 + dy), "HS TRACKER", font=title, fill=fill, anchor="lm")
    d.text((466, 304), "session tracker for Hero Siege", font=line, fill=DIM, anchor="lm")

    star = Image.open(ART / "satanic_star.png").convert("RGBA")
    star = star.resize((star.width * 2, star.height * 2), Image.NEAREST)
    for i in range(3):
        img.alpha_composite(star, (466 + i * 60, 346))

    for x, y, size in heap(w, h):
        img.alpha_composite(coin(size), (x, y))
    return img


def rounded(art: Image.Image, span: int) -> Image.Image:
    """Artwork for Discord: 1024 square, the picture inset so that a round crop
    cannot reach it. Discord shows an application icon as a circle, and the
    largest square inside a circle is the side over root two."""
    canvas = Image.new("RGBA", (1024, 1024), BLACK)
    art = art.resize((span, span), Image.NEAREST)
    canvas.alpha_composite(art, ((1024 - span) // 2, (1024 - span) // 2))
    return canvas


def write_ico(path: Path, images: list[Image.Image]) -> None:
    """An .ico is a directory of PNGs; writing it by hand keeps every size the
    nearest-neighbour one we drew, which Pillow's own resizing would soften."""
    blobs = []
    for img in images:
        buf = BytesIO()
        img.save(buf, format="PNG")
        blobs.append(buf.getvalue())
    offset = 6 + 16 * len(blobs)
    out = bytearray(struct.pack("<HHH", 0, 1, len(blobs)))
    for img, blob in zip(images, blobs):
        w = 0 if img.width >= 256 else img.width
        h = 0 if img.height >= 256 else img.height
        out += struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
    for blob in blobs:
        out += blob
    path.write_bytes(bytes(out))


def main() -> None:
    base = mark()
    ICONS.mkdir(parents=True, exist_ok=True)

    at(512, base).save(ICONS / "icon.png")
    at(32, base).save(ICONS / "32x32.png")
    at(128, base).save(ICONS / "128x128.png")
    at(256, base).save(ICONS / "128x128@2x.png")
    # the tray draws it small; 32 is what both Windows and the AppIndicator want
    at(32, base).save(ICONS / "tray.png")
    write_ico(ICONS / "icon.ico", [at(s, base) for s in ICO_SIZES])
    print(f"icons -> {ICONS}")

    if "--discord" in sys.argv:
        # Rich Presence names an asset after the file it was uploaded from, so
        # these filenames are the keys the app asks for.
        out = ICONS / "discord"
        out.mkdir(exist_ok=True)
        rounded(base, 704).save(out / "logo.png")
        star = Image.open(ART / "satanic_star.png").convert("RGBA")
        rounded(star, 621).save(out / "satanic.png")
        cover(base).save(out / "cover.png")
        print(f"discord artwork -> {out}")

    if "--preview" in sys.argv:
        sizes = [16, 20, 24, 32, 48, 64]
        pad = 12
        sheet = Image.new(
            "RGBA", (sum(s + pad for s in sizes) + pad, 64 + pad * 2), (32, 32, 36, 255)
        )
        x = pad
        for size in sizes:
            small = at(size, base)
            sheet.paste(small, (x, (sheet.height - size) // 2), small)
            x += size + pad
        out = ICONS / "preview.png"
        sheet.save(out)
        print(f"preview -> {out}")


if __name__ == "__main__":
    main()
