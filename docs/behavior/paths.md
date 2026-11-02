# Paths: Pen and Subselection tools, rectangle primitives

## Pen tool (P)

* Click: a corner point. Click and drag: a curve point (the drag sets the outgoing handle; the
  incoming one mirrors it). Shift: the new point at 45-degree steps from the last one.
* Click the first point to close the path (drag to shape the closing curve). Click the last
  point again, double-click, press Enter or Escape, or choose another tool to finish an open
  path.
* On the selected path: click an end point of an open path to continue drawing from it; click
  a segment to add a point there (the curve keeps its shape; on a straight segment the new
  point is a corner); click a curve point to make it a corner; click a corner point to delete
  it (Fireworks MX: "click a corner point once or a curve point twice").
* New paths use the Tools panel's fill and stroke, as the shape tools do.

## Subselection tool (A)

* Shows the points of the selected paths: hollow squares, filled when selected; the selected
  points' handles as lines with round ends.
* Click a point to select it (Shift: add or remove); drag to move the selected points (Shift:
  horizontally or vertically only). Arrow keys nudge them (Shift: 10 px); Delete removes them
  (a path left without points is deleted).
* Drag a handle to bend the curve: a smooth point keeps its other handle in line (each keeps its
  length); Alt moves just the one handle.
* Alt+drag a point to pull curve handles out of it.
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
