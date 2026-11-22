// Runs before FireCraft itself, as a plain script, so that even a browser too old for the app
// reads it. Three jobs:
//
// 0. Choosing between the modern WebAssembly build and the one for older browsers.
// 1. Stand-ins for the few browser features newer than Safari 13 (and a couple newer than
//    Safari 12.1) that eframe or FireCraft call. Each is only installed when it's missing.
// 2. A message when FireCraft can't start or stops with an error, instead of "Loading
//    FireCraft…" forever or a frozen page. FireCraft's Rust side calls `firecraftFailed` too
//    (start-up errors and panics).
//
// Written in old-style JavaScript (var, function, no arrows) on purpose: it has to parse in any
// browser, including ones the app can't run in.
(function () {
  "use strict";
  var w = window;

  // ---------------------------------------------------------------------------------------
  // Stand-ins

  // Array.prototype.at: Safari 15.4. js_sys::Array::at (eframe's canvas resize code).
  if (typeof Array.prototype.at !== "function") {
    Object.defineProperty(Array.prototype, "at", {
      configurable: true,
      writable: true,
      value: function (i) {
        i = Math.trunc(i) || 0;
        if (i < 0) i += this.length;
        return i < 0 || i >= this.length ? undefined : this[i];
      },
    });
  }

  // globalThis and queueMicrotask: Safari 12.1.
  if (typeof globalThis === "undefined") w.globalThis = w;
  if (typeof w.queueMicrotask !== "function") {
    w.queueMicrotask = function (callback) {
      Promise.resolve().then(callback).catch(function (e) {
        setTimeout(function () {
          throw e;
        });
      });
    };
  }

  // Blob.arrayBuffer and Blob.text: Safari 14. Opening and dropping files.
  function readBlob(blob, how) {
    return new Promise(function (resolve, reject) {
      var reader = new FileReader();
      reader.onload = function () {
        resolve(reader.result);
      };
      reader.onerror = function () {
        reject(reader.error);
      };
      reader[how](blob);
    });
  }
  if (w.Blob && typeof Blob.prototype.arrayBuffer !== "function") {
    Blob.prototype.arrayBuffer = function () {
      return readBlob(this, "readAsArrayBuffer");
    };
  }
  if (w.Blob && typeof Blob.prototype.text !== "function") {
    Blob.prototype.text = function () {
      return readBlob(this, "readAsText");
    };
  }

  // MediaQueryList.addEventListener: Safari 14 (before that only addListener). eframe watches
  // the screen's pixel ratio and the light/dark setting this way, and stops if it can't.
  if (w.MediaQueryList && typeof MediaQueryList.prototype.addEventListener !== "function") {
    MediaQueryList.prototype.addEventListener = function (type, listener, options) {
      if (type !== "change") return;
      var list = this;
      var handler = listener;
      if (options && typeof options === "object" && options.once) {
        handler = function (event) {
          list.removeListener(handler);
          listener.call(list, event);
        };
      }
      list.addListener(handler);
    };
    MediaQueryList.prototype.removeEventListener = function (type, listener) {
      if (type === "change") this.removeListener(listener);
    };
  }

  // ResizeObserver: Safari 13.1. eframe sizes the canvas from it (and draws the first frame when
  // it first reports). This one checks the observed elements' sizes when the window resizes and
  // a few times a second, and reports only the legacy `contentRect`, which eframe understands.
  if (typeof w.ResizeObserver !== "function") {
    var ResizeObserverEntry = function ResizeObserverEntry(target) {
      var r = target.getBoundingClientRect();
      this.target = target;
      this.contentRect = {
        x: 0, y: 0, top: 0, left: 0,
        width: r.width, height: r.height, right: r.width, bottom: r.height,
      };
    };
    var ResizeObserver = function ResizeObserver(callback) {
      var self = this;
      this._callback = callback;
      this._targets = [];
      this._sizes = [];
      this._timer = null;
      this._check = function () {
        self._report();
      };
    };
    ResizeObserver.prototype.observe = function (target) {
      if (this._targets.indexOf(target) >= 0) return;
      this._targets.push(target);
      this._sizes.push(null);
      if (this._timer === null) {
        w.addEventListener("resize", this._check);
        this._timer = setInterval(this._check, 250);
      }
      // Like the real thing, report the starting size soon after `observe`.
      requestAnimationFrame(this._check);
    };
    ResizeObserver.prototype.unobserve = function (target) {
      var i = this._targets.indexOf(target);
      if (i < 0) return;
      this._targets.splice(i, 1);
      this._sizes.splice(i, 1);
      if (this._targets.length === 0) this.disconnect();
    };
    ResizeObserver.prototype.disconnect = function () {
      this._targets = [];
      this._sizes = [];
      if (this._timer !== null) {
        w.removeEventListener("resize", this._check);
        clearInterval(this._timer);
        this._timer = null;
      }
    };
    ResizeObserver.prototype._report = function () {
      var entries = [];
      for (var i = 0; i < this._targets.length; i++) {
        var entry = new ResizeObserverEntry(this._targets[i]);
        var size = entry.contentRect.width + "x" + entry.contentRect.height;
        if (size !== this._sizes[i]) {
          this._sizes[i] = size;
          entries.push(entry);
        }
      }
      if (entries.length) this._callback.call(this, entries, this);
    };
    w.ResizeObserver = ResizeObserver;
    w.ResizeObserverEntry = ResizeObserverEntry;
  }

  // navigator.clipboard: Safari 13.1. eframe calls it without checking, which would stop the
  // app; this copies text the old way and turns everything else down politely.
  if (w.navigator && !navigator.clipboard) {
    var unsupported = function () {
      return Promise.reject(new Error("This browser doesn't let pages use the clipboard."));
    };
    var copyText = function (text) {
      var focused = document.activeElement;
      var area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.top = "-1000px";
      document.body.appendChild(area);
      area.select();
      var copied = false;
      try {
        copied = document.execCommand("copy");
      } catch (e) {
        copied = false;
      }
      document.body.removeChild(area);
      if (focused && focused.focus) focused.focus();
      return copied ? Promise.resolve() : unsupported();
    };
    try {
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText: copyText, readText: unsupported, write: unsupported, read: unsupported },
      });
    } catch (e) {
      // Leave it missing.
    }
  }

  // Pointer events: Safari 13. eframe takes mouse button presses from pointerdown/pointerup;
  // older browsers get them made from mousedown/mouseup (which carry everything eframe reads).
  if (typeof w.PointerEvent !== "function") {
    var lastTouch = 0;
    w.addEventListener("touchstart", function () {
      lastTouch = Date.now();
    }, true);
    var forward = function (mouseType, pointerType) {
      w.addEventListener(mouseType, function (e) {
        // A tap also sends mouse events; eframe has already seen the touch.
        if (Date.now() - lastTouch < 1000) return;
        var init = {
          bubbles: true, cancelable: true, view: w, detail: e.detail,
          screenX: e.screenX, screenY: e.screenY, clientX: e.clientX, clientY: e.clientY,
          ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey,
          button: e.button, buttons: e.buttons, relatedTarget: e.relatedTarget,
        };
        var pointer = new MouseEvent(pointerType, init);
        e.target.dispatchEvent(pointer);
        if (pointer.defaultPrevented) e.preventDefault();
      }, true);
    };
    forward("mousedown", "pointerdown");
    forward("mouseup", "pointerup");
  }

  // ---------------------------------------------------------------------------------------
  // Which WebAssembly build to load (the loader post_build.py writes reads this):
  //   "modern": built with the WebAssembly features Rust uses by default; faster.
  //   "legacy": plain "MVP" WebAssembly (in legacy/), for browsers without them.
  // Each probe is a tiny module that uses just that feature; the modern build is only for a
  // browser that accepts all of them. post_build.py checks the modern build uses nothing more,
  // so keep the two lists the same. `?wasm=modern` or `?wasm=legacy` in the address picks one
  // (for testing and timing).

  var WASM_FEATURES = {
    "sign-ext": [0, 97, 115, 109, 1, 0, 0, 0, 1, 6, 1, 96, 1, 127, 1, 127, 3, 2, 1, 0, 10, 7, 1, 5, 0, 32, 0, 192, 11],
    "nontrapping-float-to-int": [0, 97, 115, 109, 1, 0, 0, 0, 1, 6, 1, 96, 1, 125, 1, 127, 3, 2, 1, 0, 10, 8, 1, 6, 0, 32,
      0, 252, 0, 11],
    "bulk-memory": [0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 5, 3, 1, 0, 1, 10, 14, 1, 12, 0, 65, 0, 65,
      0, 65, 0, 252, 10, 0, 0, 11],
    "multivalue": [0, 97, 115, 109, 1, 0, 0, 0, 1, 6, 1, 96, 0, 2, 127, 127, 3, 2, 1, 0, 10, 8, 1, 6, 0, 65, 0, 65, 0, 11],
    "reference-types": [0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 1, 111, 0, 3, 2, 1, 0, 10, 4, 1, 2, 0, 11],
    "mutable-globals": [0, 97, 115, 109, 1, 0, 0, 0, 2, 8, 1, 1, 97, 1, 98, 3, 127, 1],
  };

  var wasmBuild = "modern";
  var lacking = [];
  try {
    for (var feature in WASM_FEATURES) {
      if (!WebAssembly.validate(new Uint8Array(WASM_FEATURES[feature]))) lacking.push(feature);
    }
  } catch (e) {
    lacking.push("WebAssembly");
  }
  if (lacking.length) wasmBuild = "legacy";
  var forced = /[?&]wasm=(modern|legacy)(&|$)/.exec(location.search);
  if (forced) wasmBuild = forced[1];
  w.firecraftWasmBuild = wasmBuild;

  // ---------------------------------------------------------------------------------------
  // What the browser is missing outright (checked now, reported if anything goes wrong).

  var missing = [];
  if (!("noModule" in document.createElement("script"))) missing.push("JavaScript modules");
  try {
    // The 8-byte header of an empty WebAssembly module.
    if (!WebAssembly.validate(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]))) throw 0;
  } catch (e) {
    missing.push("WebAssembly");
  }
  try {
    var canvas = document.createElement("canvas");
    var gl = canvas.getContext("webgl2") || canvas.getContext("webgl");
    if (!gl) throw 0;
    var lose = gl.getExtension("WEBGL_lose_context");
    if (lose) lose.loseContext();
  } catch (e) {
    missing.push("WebGL (3D graphics; it may be turned off in the browser's settings)");
  }

  // ---------------------------------------------------------------------------------------
  // The message

  var failed = false;

  // FireCraft's Rust code removes "Loading FireCraft…" once the app is running. (Before the
  // page has been read in, it isn't there yet either.)
  function starting() {
    return document.readyState === "loading" || !!document.getElementById("firecraft_loading");
  }

  function describe(reason) {
    if (reason === undefined || reason === null) return "Unknown error";
    if (typeof reason === "string") return reason;
    var text = String(reason);
    if (reason.stack && reason.stack.indexOf(text) < 0) text += "\n" + reason.stack;
    else if (reason.stack) text = reason.stack;
    return text;
  }

  function element(tag, css, text) {
    var e = document.createElement(tag);
    e.style.cssText = css;
    if (text) e.appendChild(document.createTextNode(text));
    return e;
  }

  function show(wasStarting, details) {
    var box = element("div", "position:absolute;top:0;right:0;bottom:0;left:0;overflow:auto;" +
      "background:#808080;z-index:10;font:14px/1.45 -apple-system,system-ui,'Segoe UI',sans-serif;color:#111");
    var panel = element("div", "max-width:36em;margin:8vh auto;padding:18px 22px;background:#d6d3ce;" +
      "border:1px solid #404040;box-shadow:2px 2px 0 #404040");
    box.appendChild(panel);
    var title = wasStarting ? "FireCraft couldn't start in this browser" : "FireCraft stopped because of an error";
    panel.appendChild(element("h1", "font-size:17px;margin:0 0 10px", title));
    if (missing.length) {
      panel.appendChild(element("p", "margin:0 0 10px", "This browser is missing: " + missing.join(", ") + "."));
    }
    if (wasStarting) {
      panel.appendChild(element("p", "margin:0 0 10px",
        "FireCraft needs a browser from about 2019 or later: Safari 13 or newer, or a recent Firefox, Chrome or Edge."));
    } else {
      panel.appendChild(element("p", "margin:0 0 10px", "Reload the page to start FireCraft again."));
    }
    panel.appendChild(element("p", "margin:0 0 10px",
      "If you think this is a bug, please report it at github.com/andrewjp33/firecraft/issues " +
      "and include the details below."));
    var pre = element("pre", "margin:0 0 12px;padding:8px;max-height:16em;overflow:auto;background:#fff;" +
      "border:1px solid #808080;font:12px/1.4 Menlo,Consolas,monospace;white-space:pre-wrap;word-wrap:break-word",
      details + "\n\nBrowser: " + navigator.userAgent + "\nBuild: " + wasmBuild +
      (lacking.length ? " (no " + lacking.join(", ") + ")" : ""));
    panel.appendChild(pre);
    var reload = element("button", "font:inherit;padding:3px 14px", "Reload");
    reload.onclick = function () {
      location.reload();
    };
    panel.appendChild(reload);
    document.body.appendChild(box);
    var loading = document.getElementById("firecraft_loading");
    if (loading) loading.style.display = "none";
  }

  // The first problem is the one shown; later ones are usually its consequences.
  w.firecraftFailed = function (reason) {
    if (failed) return;
    failed = true;
    var wasStarting = starting();
    var details = describe(reason);
    if (document.body) show(wasStarting, details);
    else document.addEventListener("DOMContentLoaded", function () { show(wasStarting, details); });
  };

  w.addEventListener("error", function (e) {
    var target = e.target;
    if (target && target !== w && target.tagName) {
      // A file that didn't load (these don't bubble; listening while capturing catches them).
      if (target.tagName === "SCRIPT" && starting()) {
        w.firecraftFailed("Couldn't load " + (target.src || "a script"));
      }
      return;
    }
    var message = e.message || "";
    // Harmless, and reported as an error by some browsers.
    if (/ResizeObserver loop/.test(message)) return;
    var file = e.filename || "";
    // Once running, only FireCraft's own errors (not, say, a browser extension's) count.
    if (!starting() && file && file.indexOf(location.origin) !== 0) return;
    if (e.error) w.firecraftFailed(e.error);
    else w.firecraftFailed(message + (file ? "\n  at " + file + ":" + e.lineno + ":" + e.colno : ""));
  }, true);

  w.addEventListener("unhandledrejection", function (e) {
    // While starting, anything failing means it won't start. Afterwards a rejected promise is
    // some background task failing (FireCraft reports its own), not the app stopping.
    if (starting()) w.firecraftFailed(e.reason);
  });

  if (missing.length) w.firecraftFailed("Missing: " + missing.join(", "));
})();
