#!/usr/bin/env python3
"""Capture the current X cursor (XFixes) and composite it onto a screenshot.

Screenshots of an X server (ImageMagick `import`, `xwd`) don't include the mouse cursor. This
asks the server for the cursor image and position and pastes it in, so cursor changes can be
checked in automated UI tests.

Usage: xcursor.py SCREENSHOT.png OUT.png    (needs DISPLAY; uses ImageMagick `convert`)
       xcursor.py --dump CURSOR.png        (just the cursor image)
"""
import ctypes, struct, subprocess, sys, zlib


class XFixesCursorImage(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_short), ("y", ctypes.c_short),
        ("width", ctypes.c_ushort), ("height", ctypes.c_ushort),
        ("xhot", ctypes.c_ushort), ("yhot", ctypes.c_ushort),
        ("cursor_serial", ctypes.c_ulong),
        ("pixels", ctypes.POINTER(ctypes.c_ulong)),
        ("atom", ctypes.c_ulong), ("name", ctypes.c_char_p),
    ]


def grab():
    x11 = ctypes.CDLL("libX11.so.6")
    xf = ctypes.CDLL("libXfixes.so.3")
    x11.XOpenDisplay.restype = ctypes.c_void_p
    xf.XFixesGetCursorImage.restype = ctypes.POINTER(XFixesCursorImage)
    d = x11.XOpenDisplay(None)
    if not d:
        sys.exit("cannot open display")
    img = xf.XFixesGetCursorImage(ctypes.c_void_p(d)).contents
    w, h = img.width, img.height
    rgba = bytearray()
    for i in range(w * h):
        argb = img.pixels[i] & 0xFFFFFFFF  # premultiplied ARGB in an unsigned long
        a, r, g, b = argb >> 24, (argb >> 16) & 255, (argb >> 8) & 255, argb & 255
        if a:  # un-premultiply
            r, g, b = (min(255, c * 255 // a) for c in (r, g, b))
        rgba += bytes((r, g, b, a))
    return img.x - img.xhot, img.y - img.yhot, w, h, bytes(rgba), (img.name or b"").decode()


def write_png(path, w, h, rgba):
    raw = b"".join(b"\0" + rgba[y * w * 4:(y + 1) * w * 4] for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


if __name__ == "__main__":
    x, y, w, h, rgba, name = grab()
    if sys.argv[1:2] == ["--dump"]:
        write_png(sys.argv[2], w, h, rgba)
        print(f"cursor {w}x{h} name={name!r}")
    else:
        shot, out = sys.argv[1], sys.argv[2]
        tmp = out + ".cursor.png"
        write_png(tmp, w, h, rgba)
        subprocess.run(["convert", shot, tmp, "-geometry", f"+{x}+{y}", "-composite", out], check=True)
        print(f"cursor {w}x{h} at {x},{y} name={name!r}")
