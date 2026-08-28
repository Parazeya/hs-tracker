"""Sprite index for Hero Siege's data.win (GMS2, YYC, bytecode 17).

Chunks used: STRG (strings), TPAG (frame rects), SPRT (sprites), TGIN
(texture groups -> .yytex file names), OBJT (objects, and the sprite each one
wears), ROOM (rooms, and which objects stand in them). Sprite entries carry GMS2
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
        self._parse_objt()
        self._rooms = None       # filled by rooms_with, which is not cheap

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
        """Texture group -> the `.yytex` files beside the game.

        The entry is 52 bytes: name, directory, extension, load type, then five
        POINTERS — to the texture pages, the sprites, the spine sprites, the
        fonts and the tilesets — and each points at a count followed by that
        many ids. This read the count and the ids inline instead, out of the
        middle of the pointer block, and got a garbage length: it claimed 21,794
        texture pages where the game ships 324, and only 96 of the names it
        produced were files that exist. Every sprite outside those 96 pages then
        failed to load, which is a whole-file failure that shows up one sprite
        at a time.

        The mapping is checked by the thing it describes: 325 pages come out,
        324 of them files on disk, and the one that is not is GameMaker's own
        fallback texture, which is built in rather than shipped.
        """
        base, _ = self.chunks["TGIN"]
        self.page_files = {}
        for ptr in self._ptr_list(base + 4):  # chunk starts with a version u32
            name_ptr, dir_ptr = struct.unpack_from("<II", self.raw, ptr)
            name = self.string(name_ptr)
            sub = self.string(dir_ptr) if dir_ptr else ""
            # the directory is "dyntex" for the built-in group, which is not a folder
            if sub == "dyntex":
                sub = ""
            pages_ptr = self.u32(ptr + 16)
            if not pages_ptr:
                continue
            n = self.u32(pages_ptr)
            if n > 4000:  # not a count; the layout has moved again
                continue
            ids = struct.unpack_from(f"<{n}I", self.raw, pages_ptr + 4)
            for i, page_id in enumerate(ids):
                self.page_files[page_id] = (Path(sub) if sub else Path()) / f"{name}_{i}"

    _page_cache = {}

    def _parse_objt(self):
        """Object name -> the sprite it is drawn with.

        Worth having because a boss is rarely one sprite. Most are assembled
        from parts — `Anubis_Left_Upper_Arm_spr`, `Satan_Jaw_spr` — and searching
        the names for a whole creature finds a limb, a prop or an effect instead.
        The object says which sprite is the creature, and it is the game saying
        it: `Uber_Luna_obj` wears `Fortune_Teller_Head_spr`, which no search for
        "Luna" would ever have turned up.

        Entry layout is name, sprite index, and the flags after it are not read.
        A sprite index of -1 means the object draws itself, and is left as None.

        The index is into SPRT's own order, which `self.sprites` keeps: it is
        filled from the chunk's pointer list, and a dict holds its insertion
        order.
        """
        base, _ = self.chunks["OBJT"]
        by_index = list(self.sprites)
        self.objects = {}
        for ptr in self._ptr_list(base):
            name = self.string(self.u32(ptr))
            idx = struct.unpack_from("<i", self.raw, ptr + 4)[0]
            self.objects[name] = by_index[idx] if 0 <= idx < len(by_index) else None

    def rooms_with(self, obj):
        """Every room an object is placed in, by name.

        Read on the first ask rather than at startup: it walks every instance of
        every room, which is a second or so, and most callers never want it.

        It answers "is this the same drop said twice?". The game's tables name a
        source both as a zone and as the thing you kill in it — Sheeponia and
        The Sheep King — and an object that stands in exactly one room stands in
        exactly one zone.

        The room entry is name, caption, size, then five pointers; the third of
        them is the instance list, and each instance carries its object's index
        at +8.
        """
        if self._rooms is None:
            self._rooms = {}
            names = [self.string(self.u32(p)) for p in self._ptr_list(self.chunks["OBJT"][0])]
            for ptr in self._ptr_list(self.chunks["ROOM"][0]):
                room = self.string(self.u32(ptr))
                lst = self.u32(ptr + 48)
                if not 0 < lst < len(self.raw):
                    continue
                for q in self._ptr_list(lst):
                    idx = struct.unpack_from("<i", self.raw, q + 8)[0]
                    if 0 <= idx < len(names):
                        self._rooms.setdefault(names[idx], []).append(room)
        return self._rooms.get(obj, [])

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
