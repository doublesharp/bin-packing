# Native irregular nesting

The `irregular` Cargo feature exposes `bin_packing::irregular`. It is enabled by
default and uses safe Rust geometry from `geo`; PackingSolver is a reference and
benchmark, with no C++ runtime backend.

`solve_irregular(problem, options)` returns source-indexed placements and explicit
unplaced quantities. `validate_irregular(&problem, &solution)` reconstructs each
original shape and checks the result independently of the placement search.

## Geometry and transforms

`Shape2D` contains rigid `Polygon2D` components in a common local coordinate frame.
Each component has an exterior and optional holes. Convex and concave outlines,
holes, and separated rigid components participate in actual material containment
and intersection predicates. Bounding rectangles are only inexpensive rejection
checks and candidate anchors. A part can occupy another part's hole, and
complementary concave outlines can interlock despite overlapping bounds.

`Contour2D` supports polygons, circles, and closed sequences of straight segments
and circular arcs. Polygon closing vertices are optional; curved contours must
close explicitly. Arc direction is clockwise or counterclockwise about the given
center. Equal start and end points denote a full circle. Ring winding is
normalized internally. Source material area uses analytic line/arc integration;
holes subtract material area.

Source rings must be simple and nondegenerate. Hole interiors must lie within
their component with strictly positive exterior clearance and cannot overlap
other holes. A hole touching its exterior at one point or along an edge is invalid. Components cannot overlap.
Line/arc self-intersections, including overlapping circular arcs, are checked
before polygon offset operations. Nonfinite coordinates, mismatched arc radii,
invalid topology, and excessive geometry return structured `InvalidConstraint`
errors.

JSON input models and options reject unknown fields, including nested contours,
points, and rotation ranges. An unrecognized requested physical rule therefore
fails deserialization instead of silently taking a default.

Rotations are inclusive intervals in degrees, within `0..=360`. Equal endpoints
specify discrete rotations. Continuous intervals are sampled at their endpoints,
midpoint, and equally spaced intermediate angles with a gap no greater than
`rotation_step_degrees`. This is a heuristic angular search, not an exact
continuous-rotation optimizer. If the requested samples exceed
`max_orientations`, the input is rejected rather than silently coarsened. For a
single circle, rotation and reflection are geometrically redundant, so one legal
angle is sufficient; every declared interval is still validated.

An output placement first negates the original local X coordinate when
`mirrored` is true, then rotates counterclockwise about the source origin, then
adds `x` and `y`. Translations are not necessarily bounding-box origins. The
validator checks the reported angle against the original intervals and rejects
unauthorized reflection.

Bins use the same shape model, so they can have concavity, holes, circular arcs,
and multiple material islands. Defects use bin-local coordinates and are unusable
material shapes. Rectangular defects are ordinary four-vertex polygons. Defects
must be wholly covered by the conservative usable bin at the chosen tolerance;
an exclusion crossing the outer boundary should instead be expressed in the bin
outline. Overlapping defects are permitted.

`item_spacing` measures minimum Euclidean distance between material belonging to
different items, including material around holes. `defect_spacing` measures
distance to defect material. Effective boundary clearance is the maximum of
`bin_spacing` and the source bin's `spacing`; it applies to outer boundaries,
holes, and component boundaries. Contact at zero spacing is allowed, but material
interior overlap is rejected.

## Conservative curves

Each source arc is sampled with maximum chord deviation of
`curve_tolerance / 4`. Curved item and defect geometry is then expanded by twice
`curve_tolerance`; curved bins are inset by twice that value. Offset round joins
use a maximum angular step of 0.05 radians. The offset margin exceeds the chord
and join approximation errors, so curved item outlines are never used as unsafe
inscribed polygons. Curved holes in items shrink, while holes in curved bins
expand. The approximation can reject a valid tight fit, and tight curved fits may
need a smaller tolerance or rescaled coordinates.

When both interacting shapes use a single `Circle` contour without holes, containment,
spacing, and non-overlap use their original centers and radii analytically.
Equal circles therefore fit exactly, and circle-to-circle tangency is allowed.
Mixed circle/polygon or composite-shape predicates use conservative geometry.
Reported circle bounds use the original radius; other bounds use the conservative
envelope. Material area always refers to the original source geometry.

For curved shapes, tolerance must be at least the maximum of `1e-8`, the shape's
largest span times `1e-7`, and its largest absolute sampled coordinate times
`1e-9`. Separate rings and components of a curved shape must have sampled
boundary separation greater than four tolerances. Uncertain topology at this
resolution is rejected. All predicates use finite-precision floating point;
the tolerance and coordinate limits are part of the geometry contract.

## Objectives and bounded work

| Objective | Ranking and constraints |
| --- | --- |
| `BinCount` | Fulfill as many required copies as possible, then minimize bin count, material cost, and used extents. |
| `TotalCost` | Fulfill as many copies as possible, then minimize material cost, bin count, and used extents. |
| `Knapsack` | Maximize placed profit, then minimize cost and bin count. Required copies may remain unplaced. |
| `Feasibility` | Seek full fulfillment within inventory; completed layouts are ranked by count, cost, and extents. An unfilled result is not an infeasibility proof. |
| `OpenDimensionX` | Seek fulfillment in one maximum bin, then minimize width and height. |
| `OpenDimensionY` | Seek fulfillment in one maximum bin, then minimize height and width. |
| `OpenDimensionXY` | Seek fulfillment in one maximum bin, then minimize width times height and their sum. |

Open objectives require exactly one declared maximum bin type and consume at
most one copy. Its original outline, holes, defects, and maximum dimensions remain
constraints. `used_width` runs from the bin's left bound to the rightmost placed
material; `used_height` runs from its bottom bound to the topmost material.
These values describe cropping from the declared bin origin, not a translation
of the bin or a separate cut certificate.

Construction orders ordinary demands by decreasing original material area and
knapsack demands by decreasing profit density. Count-oriented construction
prefers stock capacity; cost-oriented construction ranks new stock by cost per
estimated material capacity. Accepted placements always pass full geometry
checks. Candidate translations include bin and hole anchors, polygon vertex
contacts, clearance offsets, neighbor bounds, and analytic circle contact loci.
There is no integer grid restriction.

A deterministic greedy construction uses at most 32,768 geometry probes. Its
completed incumbent remains available even when `max_states` is zero. A bounded
beam search then explores placements and explicit skipped copies, retaining at
most `beam_width` states. Only completed demand traversals can replace the
incumbent. Improvement geometry probes are bounded by `max_states`, separately
from baseline work, and both counters appear in the result.

Each orientation/bin pair generates at most 16,384 raw vertex-contact proposals
before sorting, in addition to explicit bin, hole, neighbor, and circle anchors.
Contact enumeration samples at most 64 contour vertices, adds bound
corners, and evaluates at most `max_candidates_per_item` translations for that
pair. Full geometry, including every hole and component, remains in all
acceptance predicates. `search_truncated` records omitted proposals, sampled
vertices/angles, retained-state limits, or exhausted effort. Its absence is not
an optimality certificate: contact-based translation enumeration itself is a
heuristic. No result claims global optimality or proves general infeasibility.

## Input and work limits

These limits fail with `InvalidConstraint`; they are not silently clipped.

| Input | Supported bound |
| --- | --- |
| Source coordinates and arc centers | Finite, absolute value at most 1,000,000 |
| Circle/arc radius | Positive and at most 1,000,000 |
| Stocks / demand rows | 1–8 / 1–32 |
| Required copies | 1–64 per row, at most 64 in total |
| Components / holes | 1–16 components per shape, at most 32 holes per component |
| Defects | At most 32 per stock |
| Source contour | At most 2,048 vertices or curve segments, at most 2,048 sampled chords/vertices |
| Source shape | At most 8,192 sampled vertices before conservative offset |
| Prepared problem | At most 16,384 vertices across source bins, defects, and demands |
| Angular intervals / orientations | At most 128 intervals per demand; 1–128 orientations including reflection |
| Angular step | Finite, 0.1–180 degrees |
| Curve tolerance | Finite, `1e-8..=10000`, also subject to scale/topology rules above |
| Spacing | Finite, 0–1,000,000 |
| Improvement probes | `max_states` 0–200,000 |
| Beam width | 1–32 |
| Translations evaluated per orientation/bin | 1–512 |

Labels must be nonempty. Cost and profit must be finite, nonnegative, and small
enough that aggregate totals remain finite. A stock quantity of `None` is
unlimited; `Some(0)` makes that stock unavailable. Every demand quantity must be
positive. Duplicate labels remain separate source rows.

## Independent readback

The result includes original demand/bin indices, original-shape transforms,
source-row shortfalls, stock cost, placed profit, material area, extents, and
effort counters. No inferred item dimensions or user labels are used as source
identity. Validation checks legal transforms, containment, material intersection,
all clearances, demand conservation, inventory, totals, algorithm metadata, and
layout extents. Changed indices, duplicate placements, omitted shortfalls,
nonfinite transforms, illegal angles, and corrupted totals are rejected with
`InvalidSolution`. Malformed original input continues to return
`InvalidConstraint`.

Readback uses the spacing and approximation controls in `solution.options`.
Matching a saved result to a particular solve request also requires matching that
options record to the original request.

The native unit suite includes L-shape interlocking, filling a polygon hole,
rigid components, duplicate labels, polygon-bin holes, defects, clearances,
circles, conservative arcs, reflection, continuous-interval samples, objective
selection, bounded-incumbent retention, malformed arcs, corrupt result readback,
serialization, and a separate ray-crossing cell occupancy oracle for integer
polygon layouts. These establish native behavior; reference performance results
belong to the separate PackingSolver comparison report.
