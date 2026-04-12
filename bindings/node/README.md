<p align="center">
  <img src="https://raw.githubusercontent.com/doublesharp/bin-packing/main/docs/bin-packing.png" alt="bin-packing" width="200">
</p>

# @0xdoublesharp/bin-packing

Native Node.js bindings for the [`bin-packing`](https://crates.io/crates/bin-packing)
Rust crate — a cut list and bin packing optimizer for 1D cutting stock (linear
bar / pipe), 2D rectangular sheet packing, and 3D box packing.

Prebuilt binaries for Linux (x64, arm64), macOS (x64, arm64), and Windows
(x64, arm64). No Rust toolchain required.

## Install

```sh
npm install @0xdoublesharp/bin-packing
```

## Usage

```js
const { solve1d, solve2d, solve3d } = require('@0xdoublesharp/bin-packing');

// 1D cutting stock
const cuts = solve1d(
  {
    stock: [{ name: 'bar', length: 100 }],
    demands: [
      { name: 'A', length: 45, quantity: 2 },
      { name: 'B', length: 30, quantity: 2 },
    ],
  },
  { algorithm: 'auto' },
);

// 2D sheet packing
const sheets = solve2d(
  {
    sheets: [{ name: 'plywood', width: 96, height: 48 }],
    demands: [
      { name: 'panel', width: 24, height: 18, quantity: 4, can_rotate: true },
    ],
  },
  { algorithm: 'auto' },
);

// 3D box packing
const bins = solve3d(
  {
    bins: [{ name: 'crate', width: 60, height: 40, depth: 30 }],
    demands: [
      { name: 'box_a', width: 10, height: 10, depth: 10, quantity: 3 },
      { name: 'box_b', width: 8, height: 6, depth: 5, quantity: 2 },
    ],
  },
  { algorithm: 'auto' },
);
```

All three functions accept a problem object and an optional options object,
and return a solution object. Full TypeScript types are included.

## API

### `solve1d(problem, options?)`

Algorithms: `auto`, `first_fit_decreasing`, `best_fit_decreasing`,
`local_search`, `column_generation`.

### `solve2d(problem, options?)`

Algorithms: `auto`, `max_rects`, `max_rects_best_short_side_fit`,
`max_rects_best_long_side_fit`, `max_rects_bottom_left`,
`max_rects_contact_point`, `skyline`, `skyline_min_waste`, `guillotine`,
`guillotine_best_short_side_fit`, `guillotine_best_long_side_fit`,
`guillotine_shorter_leftover_axis`, `guillotine_longer_leftover_axis`,
`guillotine_min_area_split`, `guillotine_max_area_split`,
`next_fit_decreasing_height`, `first_fit_decreasing_height`,
`best_fit_decreasing_height`, `multi_start`.

### `solve3d(problem, options?)`

Algorithms: `auto`, `extreme_points`, `extreme_points_residual_space`,
`extreme_points_free_volume`, `extreme_points_bottom_left_back`,
`extreme_points_contact_point`, `extreme_points_euclidean`, `guillotine_3d`,
`guillotine_3d_best_short_side_fit`, `guillotine_3d_best_long_side_fit`,
`guillotine_3d_shorter_leftover_axis`, `guillotine_3d_longer_leftover_axis`,
`guillotine_3d_min_volume_split`, `guillotine_3d_max_volume_split`,
`layer_building`, `layer_building_max_rects`, `layer_building_skyline`,
`layer_building_guillotine`, `layer_building_shelf`, `wall_building`,
`column_building`, `deepest_bottom_left`, `deepest_bottom_left_fill`,
`first_fit_decreasing_volume`, `best_fit_decreasing_volume`, `multi_start`,
`grasp`, `local_search`, `branch_and_bound`.

### `version()`

Returns the package version string.

## WASM alternative

For browsers, Deno, Cloudflare Workers, or environments without native
binaries, see
[`@0xdoublesharp/bin-packing-wasm`](https://www.npmjs.com/package/@0xdoublesharp/bin-packing-wasm).

## License

MIT
