# PackingSolver completion

Complete the five outcomes recorded after the initial native expansion. Preserve
the original 1D/2D/3D APIs, prior changes and the safe-Rust boundary. PackingSolver
revision `376b8d1cc4aed5b171bfe1219c037c661faaa7cf` remains the reference.

All five implementation outcomes below are complete within their documented
models. The [final report](experiments/2026-10-28-packingsolver-completion.md)
records passing quality, feature, package/browser and independent review gates,
plus 68 validated reference cases. It retains measured losses, partial reference
incumbents, bounded-search limits and timing qualifications.

1. Irregular nesting: real material geometry, concavity, holes, curved contours,
   irregular bins/defects, legal rotations/mirroring and spacing. Curved geometry
   uses a declared conservative error envelope; finite search samples never imply
   a proof over every continuous angle.
2. Stack loading: matching footprints/stackability, nesting height, stack counts,
   weight above, density, payload, axle reactions and delivery-group constraints.
   Nesting is represented explicitly rather than pretending nested items are
   ordinary non-overlapping boxes.
3. Objectives: knapsack/profit, feasibility and open dimensions in addition to
   existing count/cost objectives. Requested objectives affect candidate search,
   not just the label on a count-first answer. Mandatory demand minima and
   incomplete-search reports remain explicit.
4. Staged cuts: authoritative physical trees, stage/cut-mode/orientation rules,
   cut spacing/count limits, trims/defects/kerf and production groups. Independently
   reconstruct the material partition and check every rule against the tree.
5. Quality/performance: repair the dense mixed-rectangle regression, retain all
   prior cases, and broaden comparisons with fixed synthetic and public fixtures.
   Validate the reference and Rust outputs independently; report quality losses,
   different work limits, host load and internal/process timing separately.

Use behavioral RED/GREEN at public boundaries, then strict quality and feature
checks, rebuilt Node/WASM consumer/browser tests, package verification and actual
reference runs. Keep raw artifacts on Scratch. Document bounded models and
approximation/search limits alongside capabilities; never silently ignore a
requested constraint or call truncated search globally optimal.
