#!/usr/bin/env python3
"""Find FireCraft's widgets on screen through its accessibility tree (AccessKit -> AT-SPI),
so UI tests click what they mean instead of guessing coordinates from screenshots.

Usage: uimap.py list [FILTER]          every named widget: role, name, centre, rectangle
       uimap.py where NAME [N]         centre "X Y" of the Nth (default 1st) widget named NAME
                                       (exact name first, else names containing it)
       uimap.py click NAME [N]         click it (XTest)
       uimap.py dclick NAME [N]        double-click it

Needs an AT-SPI bus with accessibility enabled in the session FireCraft runs in (see
ui_session.sh), and the app named "FireCraft".
"""
import sys

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def app():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        a = desktop.get_child_at_index(i)
        if a and "firecraft" in (a.get_name() or "").lower():
            return a
    return None


def walk(node, out, depth=0):
    if node is None or depth > 60:
        return
    try:
        name = node.get_name() or ""
        role = node.get_role_name()
        count = node.get_child_count()
    except Exception:  # widgets can vanish while walking
        return
    try:
        ext = node.get_extents(Atspi.CoordType.SCREEN)
        if name and ext.width > 0 and ext.height > 0:
            out.append((role, name, ext.x, ext.y, ext.width, ext.height))
    except Exception:  # the application node has no position
        pass
    for i in range(count):
        try:
            child = node.get_child_at_index(i)
        except Exception:
            continue
        walk(child, out, depth + 1)


def widgets():
    a = app()
    if a is None:
        sys.exit("FireCraft not found on the accessibility bus")
    out = []
    walk(a, out)
    return out


def find(name, n=1):
    ws = widgets()
    exact = [w for w in ws if w[1] == name]
    hits = exact or [w for w in ws if name.lower() in w[1].lower()]
    if len(hits) < n:
        sys.exit(f"no widget named {name!r} (#{n}); {len(hits)} found")
    role, label, x, y, w, h = hits[n - 1]
    return x + w // 2, y + h // 2


def main():
    a = sys.argv[1:]
    if not a:
        sys.exit(__doc__)
    if a[0] == "list":
        f = a[1].lower() if len(a) > 1 else ""
        for role, name, x, y, w, h in widgets():
            if f in name.lower():
                print(f"{role:16s} {name!r:40s} centre {x + w // 2},{y + h // 2}  rect {x},{y} {w}x{h}")
    elif a[0] in ("where", "click", "dclick"):
        x, y = find(a[1], int(a[2]) if len(a) > 2 else 1)
        if a[0] == "where":
            print(x, y)
        else:
            import os
            import subprocess
            xin = os.path.join(os.path.dirname(os.path.abspath(__file__)), "xin.py")
            subprocess.run([sys.executable, "-I", xin, "click" if a[0] == "click" else "dclick", str(x), str(y)], check=True)
    else:
        sys.exit(__doc__)


main()
