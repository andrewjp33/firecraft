# Paths: Pen and Subselection tools, rectangle primitives

## Pen tool (P)

Measured in Fireworks MX by performing the gestures and reading each point back through its
scripting API; `path_tool.rs`'s tests replay them with the same values.

* Click: a corner point. Click and drag: a curve point; the incoming handle mirrors the drag.
  While dragging, **Shift** snaps the handle to 45-degree steps, keeping the larger of its
  horizontal and vertical distances (a drag of 40, 25 gives 40, 40); **Alt** sets the outgoing
  handle only. Shift while clicking places the point at 45-degree steps from the last one, the
  same way (60, 15 gives 60, 0).
* The next-segment preview shows the curve the click would make.
* Click the first point to close the path. A drag there sets only its **incoming** handle (the
  closing curve) and leaves the outgoing one alone.
* Double-click to finish an open path (or press Enter or Escape). **A single click on the last
  point does not finish it**, but a drag there re-shapes it.
* Choosing a selection or vector tool and returning **resumes** the path; any other tool
  finishes it.
* **Ctrl**: the Subselection tool for as long as it's held; the drawing continues afterwards.
* On the path, selected or being drawn: **drag any point to pull out new handles** (a corner
  becomes a curve; a curve point gets new ones); click a curve point to make it a corner, and a
  corner point to delete it; click a segment to add a point (also while drawing), the curve
  keeping its shape.
* Click an end point of the selected open path to continue from it. While drawing, click an end
  point of **another open path to join it**: its points follow from the clicked end, drawing
  goes on from its far end, and the joined path takes the topmost path's attributes.
* New paths use the Tools panel's fill and stroke, as the shape tools do.

## Subselection tool (A)

* Shows the points of the selected paths: hollow squares for corners, hollow circles for curve
  points, filled squares when selected; handles as lines with round ends, for the selected
  points and the neighbouring points' near handles (draggable too).
* Click a point to select it (Shift: add or remove); drag to move the selected points (Shift:
  horizontally or vertically only). Arrow keys nudge them (Shift: 10 px); Delete removes them
  (a path left without points is deleted).
* Drag a handle to bend the curve: a smooth point keeps its other handle in line (each keeps its
  length); Alt moves just the one handle.
* Alt+drag a **selected** point to pull out its outgoing handle; on an unselected point Alt
  changes nothing.
* Click an object to select it **inside groups**: the innermost member under the pointer (a
  group's members, a masked group's content), as Fireworks' Subselection tool does; a path's
  points show. Drag the body to move it. Members can be transformed, given effects, reordered,
  duplicated and deleted within their group (see [objects.md](objects.md)); Select >
  Superselect (Ctrl+Right) goes back to the group.
* Drag on empty canvas to select the points inside the box; a click there deselects.
* Selected objects that aren't paths show their bounds.

## Rectangle primitives

Rectangles drawn with the Rectangle tool are Fireworks rectangle primitives (see
`docs/fireworks-png.md`, "Shape primitives"): the Property inspector shows "Rectangle" and its
**Rectangle roundness** (0-100%, the corner radius as a share of half the shorter side; also a
Rectangle tool option for new rectangles). Moving, scaling, rotating, skewing and distorting
keep them rectangles.

## Departures from Fireworks MX

* Editing a rectangle's points (Subselection, Pen) silently makes it a plain path; Fireworks
  asks first.
* Group, Ungroup, Break Apart and Convert to Symbol don't act on group members yet.
* Undo turns an edited rectangle back into a rectangle primitive.
* Pen and Subselection work in the active document window only (like the transform tools).
