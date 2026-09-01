"""Write src/theme.css: the app's palette, and each season's version of it.

Every chrome colour the windows draw with is a token. The default values are the
ones the app has always used, so the default skin is unchanged to the byte; a
theme is the same list with the hues moved.

Only hues move. Lightness is carried over exactly, because every contrast the
layout relies on — a label against its slab, a value against its chip — was
chosen at those lightnesses and would have to be re-tuned by hand otherwise.

The frozen tint and the reds that mean danger are not tokens: they mean the same
thing in any skin and stay literal in the components. Two colours that also mean
the same thing in any skin ARE tokens, because too many files were spelling them
out: see FIXED, written into every block unchanged rather than hue-shifted.

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

# Written into every block as they stand. A season moves hues; these two carry a
# meaning rather than a mood, so they must not move — magic find is blue and a
# Satanic drop is red whatever the skin is. Outside DEFAULT because `shift` is
# applied per family and neither belongs to one.
#
# Both are lighter than the shades they replace, which fell under the 3:1
# contrast floor for display text on a chip plate. The red is the one the overlay
# already uses for the Satanic zone, so no second near-identical red appears.
FIXED = {
    "--mf": "#7fb2ff",
    "--rar-satanic": "#ff6a6a",
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

# The plain skin, for people who would rather the app did not look like the
# game. Written out rather than shifted: `shift` only moves hues and carries
# every lightness over, so a skin that is not the default palette in other
# colours cannot be derived from it.
#
# shadcn's dark theme on a zinc base: #09090b under everything, #18181b for a
# card, #27272a for anything you type into or press, #fafafa for what is being
# read and #a1a1aa for what labels it.
#
# The colours are the smaller half. The rest is scale — 13px text where the game
# skin has 10px capitals, 12px of inset where it has 6, a 14px radius, a hairline
# of white at a tenth — and that lives in `plain.css`, with `skin.svelte.js`
# stopping the art from arriving at all.
LITERAL = {
    "plain": {
        # Text. shadcn works with two steps and no more: `foreground` for what
        # is being read and `muted-foreground` for what labels it. --bone-6 is
        # the body (29 uses) and --bone-13 the emphasis (35), so those are the
        # two that matter; the rest fill in between them.
        "--bone-3": "#a1a1aa", "--bone-4": "#a1a1aa", "--bone-5": "#b4b4bb",
        "--bone-6": "#e4e4e7", "--bone-7": "#d4d4d8", "--bone-8": "#e4e4e7",
        "--bone-9": "#f4f4f5", "--bone-10": "#f4f4f5", "--bone-11": "#fafafa",
        "--bone-12": "#fafafa", "--bone-13": "#fafafa", "--bone-14": "#ffffff",
        "--bone-15": "#ffffff",
        # --dim-1 is only ever an inner scrollbar thumb; --dim-2 is a note
        "--dim-1": "#3f3f46", "--dim-2": "#a1a1aa",
        # Edges are borders, except --edge-8 and --edge-2b, which the components
        # spend on secondary text far more often than on a line: both are the
        # muted foreground and neither can be a hairline colour.
        "--edge-1": "#27272a", "--edge-2": "#27272a", "--edge-3": "#2e2e33",
        "--edge-4": "#3f3f46", "--edge-5": "#3f3f46", "--edge-6": "#52525b",
        "--edge-7": "#52525b", "--edge-8": "#a1a1aa", "--edge-9": "#52525b",
        "--edge-1b": "#71717a", "--edge-2b": "#a1a1aa",
        # The accent, and the only hue on the window not carrying a meaning.
        # Violet because it is the one wide gap left in the wheel: red is
        # Satanic, green is Set, teal is Heroic, yellow is Angelic, rose is
        # Unholy and light blue is magic find.
        #
        # These two tokens also carried the Angelic rarity and the gold counter,
        # which the game palette could conflate because all three were the same
        # gold. A violet accent cannot, so `plain.css` gives both their own.
        "--gold-1": "#9b82f8", "--gold-2": "#a78bfa",
        # The surfaces, darkest first. --ground-1 is only ever a text shadow.
        "--ground-1": "#09090b", "--ground-2": "#09090b", "--ground-3": "#0d0d0f",
        "--ground-4": "#111113", "--ground-5": "#18181b", "--ground-6": "#18181b",
        "--ground-7": "#1c1c1f", "--ground-8": "#232326", "--ground-9": "#27272a",
        "--ground-10": "#3f3f46", "--ground-11": "#3f3f46",
    },
    "plainlight": {
        # The same skin with the light turned on. Not an inversion — the two
        # scales are not mirror images of each other: a near-black ground wants
        # its text near-white, while a white one wants ink, and the greys in
        # between step at different distances because the eye does. These are
        # shadcn's light theme on the same zinc base.
        #
        # The `bone` family means "how much this wants to be read", so on a light
        # ground it runs the other way: --bone-3 stays the faintest and --bone-13
        # the most present, but faint is now #71717a and present is #09090b.
        "--bone-3": "#67676f", "--bone-4": "#52525b", "--bone-5": "#52525b",
        "--bone-6": "#27272a", "--bone-7": "#3f3f46", "--bone-8": "#3f3f46",
        "--bone-9": "#27272a", "--bone-10": "#27272a", "--bone-11": "#18181b",
        "--bone-12": "#18181b", "--bone-13": "#09090b", "--bone-14": "#09090b",
        "--bone-15": "#000000",
        "--dim-1": "#d4d4d8", "--dim-2": "#67676f",
        "--edge-1": "#e4e4e7", "--edge-2": "#e4e4e7", "--edge-3": "#dcdce0",
        "--edge-4": "#d4d4d8", "--edge-5": "#d4d4d8", "--edge-6": "#c4c4ca",
        "--edge-7": "#c4c4ca", "--edge-8": "#67676f", "--edge-9": "#a1a1aa",
        "--edge-1b": "#a1a1aa", "--edge-2b": "#67676f",
        # #a78bfa is chosen to carry on a near-black card and measures 1.9 on
        # white, which is not a colour so much as a rumour of one. Two steps
        # deeper is the same violet and reads at 5.7.
        "--gold-1": "#6d28d9", "--gold-2": "#7c3aed",
        "--ground-1": "#ffffff", "--ground-2": "#ffffff", "--ground-3": "#fafafa",
        "--ground-4": "#f8f8f8", "--ground-5": "#ffffff", "--ground-6": "#fafafa",
        "--ground-7": "#f4f4f5", "--ground-8": "#efeff1", "--ground-9": "#e4e4e7",
        "--ground-10": "#d4d4d8", "--ground-11": "#d4d4d8",
    },
}


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
        " * Every chrome colour the app draws with is a token here. The frozen tint and",
        " * the reds that mean danger stay literal in the components, because those mean",
        " * the same thing whatever the skin is — and the last two tokens in each block",
        " * are the same for that reason: they carry a meaning, so a season leaves them",
        " * where they are.",
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
    lines += [f"  {n}: {c};" for n, c in FIXED.items()]
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
        lines += [f"  {n}: {c};" for n, c in FIXED.items()]
        lines.append("}")

    for skin, palette in LITERAL.items():
        missing = set(DEFAULT) - set(palette)
        if missing:
            raise SystemExit(f"{skin} is missing {len(missing)} tokens: {sorted(missing)}")
        lines += [
            "",
            f"/* {skin.title()}: not a season. Every value is its own, because this skin",
            "   moves the lightnesses too — see LITERAL in tools/gen_theme.py. */",
            f":root[data-theme='{skin}'] {{",
        ]
        lines += [f"  {name}: {palette[name]};" for name, _ in tokens()]
        lines += [f"  {n}: {c};" for n, c in FIXED.items()]
        lines.append("}")

    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(DEFAULT) + len(FIXED)} tokens -> {OUT}")

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
