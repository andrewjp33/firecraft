# View: zoom and display

* **Zoom counts screen pixels**: at 100% each document pixel is one pixel on the screen, as in
  Fireworks, also with display scaling (Windows 125%/150%, HiDPI screens). The canvas is placed on
  whole screen pixels and drawn without smoothing, so fine patterns (stroke textures such as
  Crosshatch, 1-pixel lines) show exactly as rendered. With display scaling the document
  therefore looks smaller at 100% than the scaled interface around it.
  (Before 2026-11-04 the zoom counted interface points, so at 125% scaling "100%" stretched
  each document pixel to 1.25 screen pixels and textures aliased.)
* Fireworks MX itself is unaware of display scaling: Windows enlarges its whole window, so at
  125% its "100%" is blurred rather than aliased.
* View > 100% (Ctrl+1), Zoom In/Out, Fit All; Ctrl+wheel zooms at the pointer; the Zoom tool
  zooms in on a click or to fit a dragged box (Alt or right button: out). New documents open
  fitted to their window, at most 100%.
