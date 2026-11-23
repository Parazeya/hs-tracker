"""Decode Hero Siege .yytex texture pages (GameMaker custom QOI).

Container: '2zoq' u16 w, u16 h, u32 inner_len, bzip2 stream.
Inner:     'fioq' u16 w, u16 h, u32 len, GM QOI stream — draft-QOI opcodes
           (INDEX, RUN_8/16, DIFF_8/16/24, COLOR) but deltas are sign-extended
           two's complement (NOT biased), the 64-entry index starts zeroed,
           and pixels are stored BGRA. Field order in ops is r, g, b, a.
Reference: UndertaleModTool's QoiConverter.cs.
"""

import bz2
import struct
import sys
import os
from pathlib import Path

from PIL import Image

GAME = Path(os.environ.get("HERO_SIEGE_BIN") or os.environ.get("HERO_SIEGE") or r"C:\Program Files (x86)\Steam\steamapps\common\HeroSiege\bin")

SE2 = [v if v < 2 else v - 4 for v in range(4)]
SE4 = [v if v < 8 else v - 16 for v in range(16)]
SE5 = [v if v < 16 else v - 32 for v in range(32)]


def decode_qoi(data: bytes, w: int, h: int):
    px_count = w * h
    out = bytearray(px_count * 4)
    index = [(0, 0, 0, 0)] * 64
    r = g = b = 0
    a = 255
    pos = 0
    p = 0
    n = len(data)
    run = 0
    while p < px_count * 4:
        if run > 0:
            run -= 1
        elif pos < n:
            b1 = data[pos]
            pos += 1
            if b1 >> 6 == 0b00:  # INDEX
                r, g, b, a = index[b1 & 0x3F]
            elif b1 >> 5 == 0b010:  # RUN_8
                run = b1 & 0x1F
            elif b1 >> 5 == 0b011:  # RUN_16
                run = ((b1 & 0x1F) << 8 | data[pos]) + 32
                pos += 1
            elif b1 >> 6 == 0b10:  # DIFF_8
                r = (r + SE2[(b1 >> 4) & 0x03]) & 0xFF
                g = (g + SE2[(b1 >> 2) & 0x03]) & 0xFF
                b = (b + SE2[b1 & 0x03]) & 0xFF
            elif b1 >> 5 == 0b110:  # DIFF_16
                b2 = data[pos]
                pos += 1
                r = (r + SE5[b1 & 0x1F]) & 0xFF
                g = (g + SE4[b2 >> 4]) & 0xFF
                b = (b + SE4[b2 & 0x0F]) & 0xFF
            elif b1 >> 4 == 0b1110:  # DIFF_24
                b2, b3 = data[pos], data[pos + 1]
                pos += 2
                r = (r + SE5[(b1 & 0x0F) << 1 | b2 >> 7]) & 0xFF
                g = (g + SE5[(b2 >> 2) & 0x1F]) & 0xFF
                b = (b + SE5[(b2 & 0x03) << 3 | b3 >> 5]) & 0xFF
                a = (a + SE5[b3 & 0x1F]) & 0xFF
            elif b1 >> 4 == 0b1111:  # COLOR
                if b1 & 0x08:
                    r = data[pos]
                    pos += 1
                if b1 & 0x04:
                    g = data[pos]
                    pos += 1
                if b1 & 0x02:
                    b = data[pos]
                    pos += 1
                if b1 & 0x01:
                    a = data[pos]
                    pos += 1
            index[(r ^ g ^ b ^ a) & 63] = (r, g, b, a)
        else:
            break
        out[p] = r
        out[p + 1] = g
        out[p + 2] = b
        out[p + 3] = a
        p += 4
    return out, pos, p // 4


def decode_file(path: Path) -> Image.Image:
    return decode_bytes(Path(path).read_bytes(), Path(path).name)


def decode_bytes(raw: bytes, label="texture") -> Image.Image:
    if len(raw) < 12 or raw[:4] != b"2zoq":
        raise ValueError(f"{label}: not a GameMaker texture")
    w, h, _ = struct.unpack_from("<HHI", raw, 4)
    inner = bz2.decompress(raw[12:])
    if len(inner) < 12 or inner[:4] != b"fioq":
        raise ValueError(f"{label}: invalid QOI payload")
    iw, ih, _ = struct.unpack_from("<HHI", inner, 4)
    if not w or not h or (iw, ih) != (w, h):
        raise ValueError(f"{label}: inconsistent texture dimensions")
    pixels, consumed, decoded = decode_qoi(inner[12:], w, h)
    tail = len(inner) - 12 - consumed
    if decoded != w * h or not 0 <= tail <= 4:
        raise ValueError(f"{label}: incomplete texture decoded={decoded}/{w*h} tail={tail}")
    return Image.frombytes("RGBA", (w, h), bytes(pixels))


if __name__ == "__main__":
    name = sys.argv[1] if len(sys.argv) > 1 else "act_1_swamp_tex_0"
    img = decode_file(GAME / f"{name}.yytex")
    out = Path(__file__).parent / f"{name}.png"
    img.save(out)
    print("saved", out)
