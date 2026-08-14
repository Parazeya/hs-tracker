"""Draw the installer's header and sidebar from the game's own UI art.

NSIS wants plain 24-bit BMPs at fixed sizes, so this bakes the panel texture,
the app icon and the title into the two images the installer shows.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).parent.parent
ART = ROOT / "src" / "assets" / "game"
OUT = ROOT / "src-tauri" / "installer"
FONT = ROOT / "src" / "assets" / "fonts" / "cookierunbold.ttf"

HEADER = (150, 57)
SIDEBAR = (164, 314)
GOLD = (195, 175, 117)
DIM = (123, 106, 99)


def nine_slice(texture: Image.Image, size: tuple[int, int], border: int = 6) -> Image.Image:
    """Stretch a bordered texture the way the app's CSS border-image does."""
    w, h = size
    tw, th = texture.size
    out = Image.new("RGBA", size)
    boxes = [
        ((0, 0, border, border), (0, 0)),
        ((tw - border, 0, tw, border), (w - border, 0)),
        ((0, th - border, border, th), (0, h - border)),
        ((tw - border, th - border, tw, th), (w - border, h - border)),
    ]
    middle = texture.crop((border, border, tw - border, th - border))
    out.paste(middle.resize((w - 2 * border, h - 2 * border)), (border, border))
    edges = [
        (texture.crop((border, 0, tw - border, border)), (w - 2 * border, border), (border, 0)),
        (texture.crop((border, th - border, tw - border, th)), (w - 2 * border, border), (border, h - border)),
        (texture.crop((0, border, border, th - border)), (border, h - 2 * border), (0, border)),
        (texture.crop((tw - border, border, tw, th - border)), (border, h - 2 * border), (w - border, border)),
    ]
    for strip, to, at in edges:
        out.paste(strip.resize(to), at)
    for box, at in boxes:
        out.paste(texture.crop(box), at)
    return out


def flatten(image: Image.Image) -> Image.Image:
    """NSIS reads plain 24-bit bitmaps — no alpha, no palette."""
    ground = Image.new("RGB", image.size, (26, 18, 18))
    ground.paste(image, mask=image.split()[3] if image.mode == "RGBA" else None)
    return ground


def draw_header() -> Image.Image:
    image = nine_slice(Image.open(ART / "chip_dark.png").convert("RGBA"), HEADER)
    icon = Image.open(ROOT / "src-tauri" / "icons" / "icon.png").convert("RGBA").resize((36, 36))
    image.paste(icon, (8, (HEADER[1] - 36) // 2), icon)
    draw = ImageDraw.Draw(image)
    draw.text((52, 14), "HS Tracker", font=ImageFont.truetype(str(FONT), 15), fill=GOLD)
    draw.text((53, 32), "Hero Siege", font=ImageFont.truetype(str(FONT), 10), fill=DIM)
    return flatten(image)


def draw_sidebar() -> Image.Image:
    image = nine_slice(Image.open(ART / "panel.png").convert("RGBA"), SIDEBAR)
    icon = Image.open(ROOT / "src-tauri" / "icons" / "icon.png").convert("RGBA").resize((104, 104))
    image.paste(icon, ((SIDEBAR[0] - 104) // 2, 40), icon)
    draw = ImageDraw.Draw(image)
    title = ImageFont.truetype(str(FONT), 20)
    line = ImageFont.truetype(str(FONT), 11)
    draw.text((SIDEBAR[0] // 2, 168), "HS Tracker", font=title, fill=GOLD, anchor="mm")
    for i, text in enumerate(("session overlay", "for Hero Siege")):
        draw.text((SIDEBAR[0] // 2, 196 + i * 16), text, font=line, fill=DIM, anchor="mm")
    star = Image.open(ART / "satanic_star.png").convert("RGBA")
    for i in range(3):
        image.paste(star, (SIDEBAR[0] // 2 - 36 + i * 26, 236), star)
    return flatten(image)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    draw_header().save(OUT / "header.bmp")
    draw_sidebar().save(OUT / "sidebar.bmp")
    print(f"header {HEADER[0]}x{HEADER[1]} and sidebar {SIDEBAR[0]}x{SIDEBAR[1]} -> {OUT}")


if __name__ == "__main__":
    main()
