#!/usr/bin/env python3
"""Read the X11 CLIPBOARD (no xclip needed): list its formats, or save one.

Usage: xclip.py owner              the X window owning the clipboard (0: nobody)
       xclip.py targets            list the formats (X atoms) the clipboard owner offers
       xclip.py get TARGET OUT     save that format's bytes to OUT
Used to see what Fireworks MX (under Wine) puts on the clipboard.
"""
import ctypes, sys, time

x = ctypes.CDLL("libX11.so.6")
x.XOpenDisplay.restype = ctypes.c_void_p
x.XInternAtom.restype = ctypes.c_ulong
x.XGetAtomName.restype = ctypes.c_char_p
x.XDefaultRootWindow.restype = ctypes.c_ulong
x.XCreateSimpleWindow.restype = ctypes.c_ulong
d = ctypes.c_void_p(x.XOpenDisplay(None))
if not d.value:
    sys.exit("no display")
atom = lambda n: x.XInternAtom(d, n.encode(), 0)
root = x.XDefaultRootWindow(d)
win = x.XCreateSimpleWindow(d, ctypes.c_ulong(root), 0, 0, 1, 1, 0, 0, 0)
CLIP, PROP, INCR = atom("CLIPBOARD"), atom("FC_CLIP"), atom("INCR")
SELECTION_NOTIFY, PROPERTY_NOTIFY = 31, 28


def wait(kind, timeout=5.0):
    ev = (ctypes.c_long * 24)()
    end = time.time() + timeout
    while time.time() < end:
        if x.XPending(d):
            x.XNextEvent(d, ev)
            if ctypes.cast(ev, ctypes.POINTER(ctypes.c_int))[0] == kind:
                return ev
        else:
            time.sleep(0.01)
    return None


def read_prop(delete=True):
    t, fmt = ctypes.c_ulong(), ctypes.c_int()
    n, after = ctypes.c_ulong(), ctypes.c_ulong()
    data = ctypes.POINTER(ctypes.c_ubyte)()
    x.XGetWindowProperty(d, ctypes.c_ulong(win), ctypes.c_ulong(PROP), 0, 1 << 28, delete, 0,
                         ctypes.byref(t), ctypes.byref(fmt), ctypes.byref(n), ctypes.byref(after),
                         ctypes.byref(data))
    size = {8: 1, 16: 2, 32: ctypes.sizeof(ctypes.c_long)}.get(fmt.value, 1)
    raw = bytes(ctypes.cast(data, ctypes.POINTER(ctypes.c_ubyte * (n.value * size))).contents) if n.value else b""
    return t.value, fmt.value, n.value, raw


def fetch(target):
    x.XSelectInput(d, ctypes.c_ulong(win), 1 << 22)  # PropertyChangeMask (for INCR)
    x.XConvertSelection(d, ctypes.c_ulong(CLIP), ctypes.c_ulong(atom(target)), ctypes.c_ulong(PROP),
                        ctypes.c_ulong(win), 0)
    x.XFlush(d)
    if wait(SELECTION_NOTIFY) is None:
        return None
    t, fmt, n, raw = read_prop()
    if t != INCR:
        return fmt, n, raw
    out = b""  # INCR: chunks until an empty one
    while True:
        if wait(PROPERTY_NOTIFY) is None:
            break
        t, fmt2, n, chunk = read_prop()
        if not chunk:
            break
        out += chunk
    return 8, len(out), out


if len(sys.argv) >= 2 and sys.argv[1] == "owner":
    x.XGetSelectionOwner.restype = ctypes.c_ulong
    print(hex(x.XGetSelectionOwner(d, ctypes.c_ulong(CLIP))))
elif len(sys.argv) >= 2 and sys.argv[1] == "targets":
    r = fetch("TARGETS")
    if not r:
        sys.exit("nothing on the clipboard")
    fmt, n, raw = r
    ids = ctypes.cast(ctypes.c_char_p(raw), ctypes.POINTER(ctypes.c_ulong))
    for i in range(n):
        print(x.XGetAtomName(d, ids[i]).decode(errors="replace"))
elif len(sys.argv) == 4 and sys.argv[1] == "get":
    r = fetch(sys.argv[2])
    if not r:
        sys.exit("no data")
    open(sys.argv[3], "wb").write(r[2])
    print(f"{len(r[2])} bytes")
else:
    sys.exit(__doc__)
