#!/usr/bin/env python3
"""Minimal XTest input driver for the oracle harness (no xdotool needed).

Usage: xin.py click X Y | rclick X Y | move X Y | down | up | key KEYSYM | kdown KEYSYM | kup KEYSYM
       | combo Control_L+q | focus | dclick X Y | type TEXT | sleep SECONDS ...
(focus: give keyboard focus to the top-level window under the pointer; without a window
manager, winit apps otherwise never see key events)
"""
import ctypes, sys, time
x11 = ctypes.CDLL("libX11.so.6"); xt = ctypes.CDLL("libXtst.so.6")
x11.XOpenDisplay.restype = ctypes.c_void_p
d = x11.XOpenDisplay(None)
x11.XStringToKeysym.restype = ctypes.c_ulong
x11.XDefaultRootWindow.restype = ctypes.c_ulong
def flush(): x11.XFlush(ctypes.c_void_p(d))
def move(x, y): xt.XTestFakeMotionEvent(ctypes.c_void_p(d), -1, x, y, 0); flush(); time.sleep(0.15)
def rclick(x, y):
    move(x, y)
    xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 3, 1, 0); flush(); time.sleep(0.08)
    xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 3, 0, 0); flush(); time.sleep(0.3)
def click(x, y):
    move(x, y)
    xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 1, 1, 0); flush(); time.sleep(0.08)
    xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 1, 0, 0); flush(); time.sleep(0.3)
def dclick(x, y):
    move(x, y)
    for _ in range(2):
        xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 1, 1, 0); flush(); time.sleep(0.03)
        xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 1, 0, 0); flush(); time.sleep(0.05)
    time.sleep(0.3)
def type_text(text):
    for ch in text:
        name = {" ": "space", "_": "underscore"}.get(ch, ch)
        key(name)
def button(down):
    xt.XTestFakeButtonEvent(ctypes.c_void_p(d), 1, 1 if down else 0, 0); flush(); time.sleep(0.15)
def keyev(name, down):
    kc = x11.XKeysymToKeycode(ctypes.c_void_p(d), x11.XStringToKeysym(name.encode()))
    xt.XTestFakeKeyEvent(ctypes.c_void_p(d), kc, 1 if down else 0, 0); flush(); time.sleep(0.1)
def key(name):
    kc = x11.XKeysymToKeycode(ctypes.c_void_p(d), x11.XStringToKeysym(name.encode()))
    xt.XTestFakeKeyEvent(ctypes.c_void_p(d), kc, 1, 0); flush(); time.sleep(0.05)
    xt.XTestFakeKeyEvent(ctypes.c_void_p(d), kc, 0, 0); flush(); time.sleep(0.3)
def focus():
    root = x11.XDefaultRootWindow(ctypes.c_void_p(d))
    r, c = ctypes.c_ulong(), ctypes.c_ulong()
    i = [ctypes.c_int() for _ in range(4)]; m = ctypes.c_uint()
    x11.XQueryPointer(ctypes.c_void_p(d), ctypes.c_ulong(root), ctypes.byref(r), ctypes.byref(c),
                      *[ctypes.byref(v) for v in i], ctypes.byref(m))
    if c.value:
        x11.XSetInputFocus(ctypes.c_void_p(d), ctypes.c_ulong(c.value), 1, 0); flush(); time.sleep(0.3)
def combo(names):
    kcs=[x11.XKeysymToKeycode(ctypes.c_void_p(d), x11.XStringToKeysym(n.encode())) for n in names]
    for k in kcs: xt.XTestFakeKeyEvent(ctypes.c_void_p(d), k, 1, 0); flush(); time.sleep(0.05)
    for k in reversed(kcs): xt.XTestFakeKeyEvent(ctypes.c_void_p(d), k, 0, 0); flush(); time.sleep(0.05)
    time.sleep(0.6)

a = sys.argv[1:]
while a:
    c = a.pop(0)
    if c == "click": click(int(a.pop(0)), int(a.pop(0)))
    elif c == "move": move(int(a.pop(0)), int(a.pop(0)))
    elif c == "rclick": rclick(int(a.pop(0)), int(a.pop(0)))
    elif c == "combo": combo(a.pop(0).split("+"))
    elif c == "kdown": keyev(a.pop(0), True)
    elif c == "kup": keyev(a.pop(0), False)
    elif c == "down": button(True)
    elif c == "up": button(False)
    elif c == "key": key(a.pop(0))
    elif c == "focus": focus()
    elif c == "dclick": dclick(int(a.pop(0)), int(a.pop(0)))
    elif c == "type": type_text(a.pop(0))
    elif c == "sleep": time.sleep(float(a.pop(0)))

