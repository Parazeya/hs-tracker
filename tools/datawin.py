"""Sprite index for Hero Siege's data.win (GMS2, YYC, bytecode 17).

Chunks used: STRG (strings), TPAG (frame rects), SPRT (sprites), TGIN
(texture groups -> .yytex file names). Sprite entries carry GMS2
version-dependent fields between the type and the frame list; TPAG frames are
packed trimmed, so render_x/y + bound_w/h must be honoured when compositing.
"""

import struct
import sys
import os
from pathlib import Path

from PIL import Image

import yytex

GAME = Path(r"F:\Games\Steam\steamapps\common\HeroSiege\bin")
DATA = GAME / "data.win"


class DataWin:
    def __init__(self, path=DATA):
        self.raw = path.read_bytes()
        assert self.raw[:4] == b"FORM"
        self.chunks = {}
        pos = 8
        while pos < len(self.raw):
            tag = self.raw[pos:pos + 4].decode("ascii")
            size = struct.unpack_from("<I", self.raw, pos + 4)[0]
            self.chunks[tag] = (pos + 8, size)
            pos += 8 + size
        self._strings = {}
        self._parse_tpag()
        self._parse_sprt()
        self._parse_tgin()

    def u32(self, pos):
        return struct.unpack_from("<I", self.raw, pos)[0]

    def string(self, ptr):
        if ptr not in self._strings:
            n = self.u32(ptr - 4)
            self._strings[ptr] = self.raw[ptr:ptr + n].decode("utf-8", "replace")
        return self._strings[ptr]

    def _ptr_list(self, pos):
        n = self.u32(pos)
        return list(struct.unpack_from(f"<{n}I", self.raw, pos + 4))

    def _parse_tpag(self):
        base, _ = self.chunks["TPAG"]
        self.tpag = {}
        for ptr in self._ptr_list(base):
            vals = struct.unpack_from("<11H", self.raw, ptr)
            self.tpag[ptr] = {
                "src": vals[0:4],          # x, y, w, h on the page
                "render": vals[4:6],       # offset inside the logical frame
                "bound": vals[8:10],       # logical frame size
                "page": vals[10],
            }

    def _parse_sprt(self):
        base, _ = self.chunks["SPRT"]
        self.sprites = {}
        for ptr in self._ptr_list(base):
            name = self.string(self.u32(ptr))
            pos = ptr + 4 + 13 * 4  # width..origin_y
            frames = []
            speed = None
            if struct.unpack_from("<i", self.raw, pos)[0] == -1:
                version = self.u32(pos + 4)
                pos += 12  # -1, version, sprite type
                speed = struct.unpack_from("<f", self.raw, pos)[0]
                pos += 8  # playback speed + unit
                pos += 4 * (version >= 2)  # sequence ptr
                pos += 4 * (version >= 3)  # nine-slice ptr
            n = self.u32(pos)
            if n < 4096:
                frames = [self.u32(pos + 4 + i * 4) for i in range(n)]
            self.sprites[name] = {"frames": frames, "speed": speed}

    def _parse_tgin(self):
        # entry: name, directory, extension, loadtype, five list ptrs
        # (texture pages, sprites, spine, fonts, tilesets); pages are ids
        base, _ = self.chunks["TGIN"]
        self.page_files = {}
        for ptr in self._ptr_list(base + 4):  # chunk starts with a version u32
            name_ptr, dir_ptr = struct.unpack_from("<II", self.raw, ptr)
            name = self.string(name_ptr)
            sub = self.string(dir_ptr) if dir_ptr else ""
            n = self.u32(ptr + 36)
            ids = struct.unpack_from(f"<{n}I", self.raw, ptr + 40)
            for i, page_id in enumerate(ids):
                self.page_files[page_id] = (Path(sub) if sub else Path()) / f"{name}_{i}"

    _page_cache = {}

    def page_image(self, page_id):
        if page_id not in self._page_cache:
            fname = self.page_files[page_id]
            self._page_cache[page_id] = yytex.decode_file(GAME / fname.with_suffix(".yytex"))
        return self._page_cache[page_id]

    def frame_image(self, tpag_ptr):
        t = self.tpag[tpag_ptr]
        page = self.page_image(t["page"])
        x, y, w, h = t["src"]
        crop = page.crop((x, y, x + w, y + h))
        out = Image.new("RGBA", t["bound"])
        out.paste(crop, t["render"])
        return out

    def sprite_frames(self, name):
        return [self.frame_image(p) for p in self.sprites[name]["frames"]]


if __name__ == "__main__":
    dw = DataWin()
    print("chunks:", " ".join(dw.chunks))
    print("sprites:", len(dw.sprites), "tpag:", len(dw.tpag), "pages:", len(dw.page_files))
    if len(sys.argv) > 1:
        pat = sys.argv[1].lower()
        for name in sorted(dw.sprites):
            if pat in name.lower():
                s = dw.sprites[name]
                print(f"{name}  frames={len(s['frames'])} speed={s['speed']}")
