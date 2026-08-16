"""Write src/theme.css: the app's palette, and each season's version of it.

Every chrome colour the windows draw with is a token. The default values are the
ones the app has always used, so the default skin is unchanged to the byte; a
theme is the same list with the hues moved.

Only hues move. Lightness is carried over exactly, because every contrast the
layout relies on — a label against its slab, a value against its chip — was
chosen at those lightnesses and would have to be re-tuned by hand otherwise.

The rarities, the frozen tint and the reds that mean danger are not tokens: they
mean the same thing in any skin and stay literal in the components.

    python tools/gen_theme.py            # rewrites src/theme.css
    python tools/gen_theme.py --preview  # also a swatch sheet to look at
"""

import colorsys
import sys
from pathlib import Path

ROOT = Path(__file__).parent.parent
OUT = ROOT / "src" / "theme.css"

# The palette as it stands, named as the components name it. Explicit rather
# than generated from a sorted list: a token's name is written into ten files,
# so it must not shift because a colour was added or dropped here.
DEFAULT = {
    "--bone-3": "#8d7d63", "--bone-4": "#9a8a68", "--bone-5": "#a99873",
    "--bone-6": "#c3af75", "--bone-7": "#b8a894", "--bone-8": "#c8b48a",
    "--bone-9": "#e0cc90", "--bone-10": "#e2cf98", "--bone-11": "#e8d8a8",
    "--bone-12": "#e8d9b0", "--bone-13": "#f0e0b0", "--bone-14": "#f0e8b0",
    "--bone-15": "#f4e6bb",
    "--dim-1": "#4a3a3a", "--dim-2": "#7b6a63",
    "--edge-1": "#4a3428", "--edge-2": "#5a3a3a", "--edge-3": "#6b4a34",
    "--edge-4": "#7a4a4a", "--edge-5": "#6b5b53", "--edge-6": "#7a6a4e",
    "--edge-7": "#8a5a5a", "--edge-8": "#8a7a5a", "--edge-9": "#8c7668",
    "--edge-1b": "#8d5a5a", "--edge-2b": "#8d5f5f",
    "--gold-1": "#e2c563", "--gold-2": "#e8c860",
    "--ground-1": "#140a0a", "--ground-2": "#1a0a0a", "--ground-3": "#180d10",
    "--ground-4": "#1b1013", "--ground-5": "#1d1414", "--ground-6": "#24151a",
    "--ground-7": "#241a1c", "--ground-8": "#2c1a1d", "--ground-9": "#3b2126",
    "--ground-10": "#3a2b2b", "--ground-11": "#3d2a2c",
}

def family(token: str) -> str:
    return token.lstrip("-").split("-")[0]


# Ebontharn, read off the season's key art: a violet ground with jade on it.
# (hue, saturation kept, the least allowed, lightness multiplier)
#
# Text is lifted a little. The warm palette read against warm brown; the same
# lightness in mint against violet is a weaker contrast than it was, and the
# labels were the first thing to become hard to read.
SEASONS = {
    "ebontharn": {
        "ground": (0.735, 1.00, 0.22, 1.00),
        "edge": (0.760, 0.90, 0.24, 1.00),
        "gold": (0.442, 1.05, 0.40, 1.00),
        "dim": (0.500, 0.60, 0.10, 1.22),
        "bone": (0.452, 1.00, 0.14, 1.10),
    },
}
# a ceiling on saturation per family, so a theme reads as a skin and not a toy
CEILING = {"bone": 0.34, "gold": 0.72, "dim": 0.30, "edge": 0.55, "ground": 0.45}


def shift(hex_colour: str, hue: float, keep: float, floor: float, lift: float, ceiling: float) -> str:
    r, g, b = (int(hex_colour[i:i + 2], 16) / 255 for i in (1, 3, 5))
    _, light, sat = colorsys.rgb_to_hls(r, g, b)
    sat = min(max(sat * keep, floor), ceiling)
    r, g, b = colorsys.hls_to_rgb(hue, min(0.96, light * lift), sat)
    return "#%02x%02x%02x" % (round(r * 255), round(g * 255), round(b * 255))


def tokens():
    """(name, default) in the order they are written, families together."""
    return DEFAULT.items()


def main() -> None:
    lines = [
        "/* The palette, in one place.",
        " *",
        " * Every chrome colour the app draws with is a token here; the rarities, the",
        " * frozen tint and the reds that mean danger stay literal in the components,",
        " * because those mean the same thing whatever the skin is.",
        " *",
        " * Written by tools/gen_theme.py — edit there, not here.",
        " */",
        "",
        "/* Every window is transparent underneath its own art, and so is the page",
        "   when it is served to an OBS Browser Source. Set here rather than in a",
        "   component, so there is never a frame of white before one mounts. */",
        "html,",
        "body {",
        "  /* Not `transparent`. WebKitGTK on X11 hands a fully transparent",
        "     page no background layer to paint, so nothing erases the frame",
        "     before it: on Linux the old text stayed under the new one every",
        "     time a number changed, and lock and unlock drew on top of each",
        "     other. One part in 255 of black is invisible on any ground and",
        "     is enough to make the whole surface repaint. */",
        "  background: rgb(0 0 0 / 0.004);",
        "}",
        "",
        "/* The scrollbar, in the same paint as everything else. The one the",
        "   webview draws is a strip of system grey down the side of a panel",
        "   made of pixel art, and it is the first thing the eye goes to. Kept",
        "   here rather than per panel: every list in the app scrolls, and one",
        "   of them wearing the system bar is the whole point missed. It is",
        "   drawn from the tokens, so a season changes it along with the rest. */",
        "* {",
        "  scrollbar-width: thin;",
        "  scrollbar-color: var(--edge-8) transparent;",
        "}",
        "::-webkit-scrollbar {",
        "  width: 10px;",
        "  height: 10px;",
        "}",
        "::-webkit-scrollbar-track {",
        "  background: rgb(0 0 0 / 0.28);",
        "}",
        "::-webkit-scrollbar-thumb {",
        "  background: var(--edge-8);",
        "  border: 2px solid transparent;",
        "  background-clip: padding-box;",
        "}",
        "::-webkit-scrollbar-thumb:hover {",
        "  background: var(--gold-2);",
        "  background-clip: padding-box;",
        "}",
        "/* the little square where the two bars meet, and the end buttons: the",
        "   webview draws arrows there that no amount of colour makes belong */",
        "::-webkit-scrollbar-corner {",
        "  background: transparent;",
        "}",
        "::-webkit-scrollbar-button {",
        "  display: none;",
        "}",
        "",
        ":root {",
    ]
    for name, colour in tokens():
        lines.append(f"  {name}: {colour};")
    lines.append("}")

    for season, rule in SEASONS.items():
        lines += [
            "",
            f"/* {season.title()}: the season's own colours. Only the hues move — every",
            "   lightness is the one above, so the contrast the layout was built around",
            "   is unchanged. */",
            f":root[data-theme='{season}'] {{",
        ]
        for name, colour in tokens():
            fam = family(name)
            hue, keep, floor, lift = rule[fam]
            lines.append(f"  {name}: {shift(colour, hue, keep, floor, lift, CEILING[fam])};")
        lines.append("}")

    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(DEFAULT)} tokens -> {OUT}")

    if "--preview" in sys.argv:
        from PIL import Image, ImageDraw

        import itertools
        rows = [("default", None)] + list(SEASONS.items())
        fams = list(dict.fromkeys(family(t) for t in DEFAULT))
        cell, pad = 46, 6
        widest = max(sum(1 for t in DEFAULT if family(t) == f) for f in fams)
        width = widest * (cell + pad) + pad
        height = len(rows) * len(fams) * (cell + pad) + pad
        sheet = Image.new("RGB", (width, height), (20, 18, 24))
        d = ImageDraw.Draw(sheet)
        y = pad
        for _, rule in rows:
            for fam in fams:
                here = [c for t, c in DEFAULT.items() if family(t) == fam]
                for i, colour in enumerate(here):
                    shown = colour if rule is None else shift(colour, *rule[fam], CEILING[fam])
                    d.rectangle([pad + i * (cell + pad), y, pad + i * (cell + pad) + cell, y + cell], fill=shown)
                y += cell + pad
        out = ROOT / "src" / "theme-preview.png"
        sheet.save(out)
        print(f"preview -> {out}")


if __name__ == "__main__":
    main()
