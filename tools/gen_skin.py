"""Recolour the game's UI sprites into a season's skin.

The palette in theme.css only reaches what CSS draws. The panels, chips, buttons
and headers are the game's own PNGs, and until they move too a theme is a change
of text colour on the same brown frames.

The rule is the one theme.css uses, applied per pixel: keep the lightness, keep
the alpha, move the hue. Dark pixels — the slab and its shadow — go violet; the
bright ones, the bronze rim and its highlights, go jade. Nothing is redrawn, so
every sprite keeps its shape, its edges and its transparency exactly.

Sprites that carry a meaning rather than a surface are left alone: the coin is
gold in any season, the satanic star is the satanic star, and the ice on a
paused overlay has to stay cold.

    python tools/gen_skin.py            # writes src/assets/game/<season>/
    python tools/gen_skin.py --preview  # also a before-and-after sheet
"""

import colorsys
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).parent.parent
ART = ROOT / "src" / "assets" / "game"

# the surfaces a skin owns
SKINNED = [
    "panel.png", "chip.png", "chip_dark.png", "header.png",
    "button.png", "button_hover.png", "button_down.png",
    "close.png", "close_hover.png", "minimize.png", "minimize_hover.png", "check_off.png", "check_on.png",
    "lock.png", "lock_pale.png", "lock_gold.png", "token.png",
]
# and the ones it does not
KEPT = ["coin_strip.png", "satanic_star.png", "frozen.png", "frozen_icon.png"]

# Scenery a season brings of its own, straight from the game and not recoloured.
# `backdrop` sits behind the dashboard; a set without one simply has no backdrop,
# which is what the default skin is.
SCENERY = {
    "ebontharn": {
        "backdrop.png": ("Abyss_Realm_Space_Background_spr", 0.5),
        "stars.png": ("Abyss_Realm_Background_Star_1_tile", 1.0),
    },
}

# (hue for the dark half, hue for the light half, how much saturation to keep,
#  the least saturation allowed, the most, how much to lift the darkest pixels)
#
# The lift matters more than it sounds. The game's chips are almost black, and at
# that lightness no hue is visible at all — recoloured without it, the surfaces
# the eye spends most of its time on stay the same black they were and the skin
# reads as a change of text colour.
SEASONS = {
    "ebontharn": (0.750, 0.442, 0.85, 0.30, 0.40, 1.55),
}
# below this lightness a pixel is the slab; above it, the rim and its highlights
SPLIT = 0.46


def reskin(image: Image.Image, rule) -> Image.Image:
    dark_hue, light_hue, keep, floor, ceiling, lift = rule
    image = image.convert("RGBA")
    out = image.copy()
    pixels = out.load()
    for y in range(out.height):
        for x in range(out.width):
            r, g, b, a = pixels[x, y]
            if a == 0:
                continue
            _, light, sat = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
            # a grey pixel has no hue worth moving — the black outlines the game
            # draws its sprites with are grey, and they must stay black
            if sat < 0.05 and light < 0.2:
                continue
            hue = light_hue if light >= SPLIT else dark_hue
            sat = min(max(sat * keep, floor), ceiling)
            # a near-black slab is lifted just enough to carry a colour at all
            if light < 0.18:
                light = min(0.30, light * lift)
            nr, ng, nb = colorsys.hls_to_rgb(hue, light, sat)
            pixels[x, y] = (round(nr * 255), round(ng * 255), round(nb * 255), a)
    return out


def main() -> None:
    made = []
    for season, rule in SEASONS.items():
        out = ART / season
        out.mkdir(parents=True, exist_ok=True)
        for name in SKINNED:
            source = ART / name
            if not source.exists():
                print(f"  missing: {name}")
                continue
            reskin(Image.open(source), rule).save(out / name)
            made.append((season, name))
        # the ones a season does not own are copied through, so a skin folder is
        # a complete set and nothing has to fall back at runtime
        for name in KEPT:
            source = ART / name
            if source.exists():
                Image.open(source).convert("RGBA").save(out / name)
        print(f"{season}: {len(SKINNED)} recoloured, {len(KEPT)} carried over -> {out}")

    if "--preview" in sys.argv and made:
        season = made[0][0]
        shown = ["panel.png", "chip_dark.png", "header.png", "button.png", "close.png", "check_on.png"]
        cell, pad = 190, 10
        sheet = Image.new("RGBA", (len(shown) * (cell + pad) + pad, 2 * (cell + pad) + pad), (26, 22, 30, 255))
        for i, name in enumerate(shown):
            for row, folder in enumerate([ART, ART / season]):
                im = Image.open(folder / name).convert("RGBA")
                k = min(cell / im.width, cell / im.height, 6)
                im = im.resize((max(1, int(im.width * k)), max(1, int(im.height * k))), Image.NEAREST)
                sheet.alpha_composite(im, (pad + i * (cell + pad), pad + row * (cell + pad)))
        out = ROOT / "src" / "skin-preview.png"
        sheet.convert("RGB").save(out)
        print(f"preview -> {out}")


if __name__ == "__main__":
    main()
