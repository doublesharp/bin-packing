/* tslint:disable */
/* eslint-disable */
//
// Hand-written TypeScript definitions for @0xdoublesharp/bin-packing-wasm.
// These override the `any`-typed declarations that wasm-bindgen generates by
// default, so callers get full IntelliSense on problems, options, and
// solutions. The runtime bindings are produced by wasm-bindgen and are
// untouched.

/** Algorithm selector for `solve1d`. */
export type OneDAlgorithm =
  | 'auto'
  | 'first_fit_decreasing'
  | 'best_fit_decreasing'
  | 'local_search'
  | 'column_generation';

/** Algorithm selector for `solve2d`. */
export type TwoDAlgorithm =
  | 'auto'
  | 'max_rects'
  | 'max_rects_best_short_side_fit'
  | 'max_rects_best_long_side_fit'
  | 'max_rects_bottom_left'
  | 'max_rects_contact_point'
  | 'skyline'
  | 'skyline_min_waste'
  | 'guillotine'
  | 'guillotine_best_short_side_fit'
  | 'guillotine_best_long_side_fit'
  | 'guillotine_shorter_leftover_axis'
  | 'guillotine_longer_leftover_axis'
  | 'guillotine_min_area_split'
  | 'guillotine_max_area_split'
  | 'next_fit_decreasing_height'
  | 'first_fit_decreasing_height'
  | 'best_fit_decreasing_height'
  | 'multi_start'
  | 'rotation_search';

/** Algorithm selector for `solve3d`. */
export type ThreeDAlgorithm =
  | 'auto'
  | 'extreme_points'
  | 'extreme_points_residual_space'
  | 'extreme_points_free_volume'
  | 'extreme_points_bottom_left_back'
  | 'extreme_points_contact_point'
  | 'extreme_points_euclidean'
  | 'guillotine_3d'
  | 'guillotine_3d_best_short_side_fit'
  | 'guillotine_3d_best_long_side_fit'
  | 'guillotine_3d_shorter_leftover_axis'
  | 'guillotine_3d_longer_leftover_axis'
  | 'guillotine_3d_min_volume_split'
  | 'guillotine_3d_max_volume_split'
  | 'layer_building'
  | 'layer_building_max_rects'
  | 'layer_building_skyline'
  | 'layer_building_guillotine'
  | 'layer_building_shelf'
  | 'wall_building'
  | 'column_building'
  | 'deepest_bottom_left'
  | 'deepest_bottom_left_fill'
  | 'first_fit_decreasing_volume'
  | 'best_fit_decreasing_volume'
  | 'multi_start'
  | 'grasp'
  | 'local_search'
  | 'branch_and_bound';

// ---------------------------------------------------------------------------
// 1D cutting stock
// ---------------------------------------------------------------------------

export interface Stock1D {
  name: string;
  length: number;
  kerf?: number;
  trim?: number;
  cost?: number;
  available?: number | null;
}

export interface CutDemand1D {
  name: string;
  length: number;
  quantity: number;
}

export interface OneDProblem {
  stock: Stock1D[];
  demands: CutDemand1D[];
}

export interface OneDOptions {
  algorithm?: OneDAlgorithm;
  multistart_runs?: number;
  improvement_rounds?: number;
  column_generation_rounds?: number;
  exact_pattern_limit?: number;
  auto_exact_max_types?: number;
  auto_exact_max_quantity?: number;
  seed?: number | null;
}

export interface CutAssignment1D {
  name: string;
  length: number;
}

export interface StockLayout1D {
  stock_name: string;
  stock_length: number;
  used_length: number;
  remaining_length: number;
  waste: number;
  cost: number;
  cuts: CutAssignment1D[];
}

export interface StockRequirement1D {
  stock_name: string;
  stock_length: number;
  usable_length: number;
  cost: number;
  available_quantity: number | null;
  used_quantity: number;
  required_quantity: number;
  additional_quantity_needed: number;
}

export interface SolverMetrics1D {
  iterations: number;
  generated_patterns: number;
  enumerated_patterns: number;
  explored_states: number;
  notes: string[];
}

export interface OneDSolution {
  algorithm: string;
  exact: boolean;
  lower_bound: number | null;
  stock_count: number;
  total_waste: number;
  total_cost: number;
  layouts: StockLayout1D[];
  stock_requirements: StockRequirement1D[];
  unplaced: CutAssignment1D[];
  metrics: SolverMetrics1D;
}

// ---------------------------------------------------------------------------
// 2D rectangular packing
// ---------------------------------------------------------------------------

export interface Sheet2D {
  name: string;
  width: number;
  height: number;
  cost?: number;
  quantity?: number | null;
  kerf?: number;
  /** When true, the trailing placement may extend up to one kerf past
   *  the sheet's right and bottom edges, modeling a cut that runs off
   *  the stock. Does not relax individual part size limits. Default: false. */
  edge_kerf_relief?: boolean;
}

export interface RectDemand2D {
  name: string;
  width: number;
  height: number;
  quantity: number;
  can_rotate?: boolean;
}

export interface TwoDProblem {
  sheets: Sheet2D[];
  demands: RectDemand2D[];
}

export interface TwoDOptions {
  algorithm?: TwoDAlgorithm;
  multistart_runs?: number;
  beam_width?: number;
  guillotine_required?: boolean;
  min_usable_side?: number;
  auto_rotation_search_max_types?: number;
  seed?: number | null;
}

export interface Placement2D {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  rotated: boolean;
}

export interface SheetLayout2D {
  sheet_name: string;
  width: number;
  height: number;
  cost: number;
  placements: Placement2D[];
  used_area: number;
  waste_area: number;
  kerf_area: number;
  largest_usable_drop_area: number;
  sum_sq_usable_drop_areas: number;
}

export interface SolverMetrics2D {
  iterations: number;
  explored_states: number;
  notes: string[];
}

export interface TwoDSolution {
  algorithm: string;
  guillotine: boolean;
  sheet_count: number;
  total_waste_area: number;
  total_kerf_area: number;
  total_cost: number;
  max_usable_drop_area: number;
  total_sum_sq_usable_drop_areas: number;
  layouts: SheetLayout2D[];
  unplaced: RectDemand2D[];
  metrics: SolverMetrics2D;
}

// ---------------------------------------------------------------------------
// 3D rectangular bin packing
// ---------------------------------------------------------------------------

export interface Bin3D {
  name: string;
  width: number;
  height: number;
  depth: number;
  cost?: number;
  quantity?: number | null;
}

export interface BoxDemand3D {
  name: string;
  width: number;
  height: number;
  depth: number;
  quantity: number;
  allowed_rotations?: number;
}

export interface ThreeDProblem {
  bins: Bin3D[];
  demands: BoxDemand3D[];
}

export interface ThreeDOptions {
  algorithm?: ThreeDAlgorithm;
  multistart_runs?: number;
  improvement_rounds?: number;
  beam_width?: number;
  seed?: number | null;
  branch_and_bound_node_limit?: number;
  guillotine_required?: boolean;
  auto_exact_max_types?: number;
  auto_exact_max_quantity?: number;
}

export interface Placement3D {
  name: string;
  x: number;
  y: number;
  z: number;
  width: number;
  height: number;
  depth: number;
  rotation: 'xyz' | 'xzy' | 'yxz' | 'yzx' | 'zxy' | 'zyx';
}

export interface BinLayout3D {
  bin_name: string;
  width: number;
  height: number;
  depth: number;
  cost: number;
  placements: Placement3D[];
  used_volume: number;
  waste_volume: number;
}

export interface BinRequirement3D {
  bin_name: string;
  bin_width: number;
  bin_height: number;
  bin_depth: number;
  cost: number;
  available_quantity: number | null;
  used_quantity: number;
  required_quantity: number;
  additional_quantity_needed: number;
}

export interface SolverMetrics3D {
  iterations: number;
  explored_states: number;
  extreme_points_generated: number;
  branch_and_bound_nodes: number;
  notes: string[];
}

export interface ThreeDSolution {
  algorithm: string;
  exact: boolean;
  lower_bound: number | null;
  guillotine: boolean;
  bin_count: number;
  total_waste_volume: number;
  total_cost: number;
  layouts: BinLayout3D[];
  bin_requirements: BinRequirement3D[];
  unplaced: BoxDemand3D[];
  metrics: SolverMetrics3D;
}

// ---------------------------------------------------------------------------
// Cut planning — 1D
// ---------------------------------------------------------------------------

export type CutPlanPreset1D = 'chop_saw';

export interface CutPlanOptions1D {
  preset?: CutPlanPreset1D;
  cut_cost?: number;
  fence_reset_cost?: number;
}

export interface EffectiveCosts1D {
  cut_cost: number;
  fence_reset_cost: number;
}

export type CutStep1D =
  | { kind: 'cut'; position: number; piece_name: string }
  | { kind: 'fence_reset'; new_position: number };

export interface BarCutPlan1D {
  stock_name: string;
  bar_index_in_solution: number;
  total_cost: number;
  num_cuts: number;
  num_fence_resets: number;
  steps: CutStep1D[];
}

export interface CutPlanSolution1D {
  preset: CutPlanPreset1D;
  effective_costs: EffectiveCosts1D;
  bar_plans: BarCutPlan1D[];
  total_cost: number;
}

// ---------------------------------------------------------------------------
// Cut planning — 2D
// ---------------------------------------------------------------------------

export type CutPlanPreset2D = 'table_saw' | 'panel_saw' | 'cnc_router';

export interface CutPlanOptions2D {
  preset?: CutPlanPreset2D;
  cut_cost?: number;
  rotate_cost?: number;
  fence_reset_cost?: number;
  tool_up_down_cost?: number;
  travel_cost?: number;
}

export interface EffectiveCosts2D {
  cut_cost: number;
  rotate_cost: number;
  fence_reset_cost: number;
  tool_up_down_cost: number;
  travel_cost: number;
}

export type CutAxis = 'vertical' | 'horizontal';

export type CutStep2D =
  | { kind: 'cut'; axis: CutAxis; position: number }
  | { kind: 'line_cut'; from_x: number; from_y: number; to_x: number; to_y: number }
  | { kind: 'rotate' }
  | { kind: 'fence_reset'; new_position: number }
  | { kind: 'tool_up' }
  | { kind: 'tool_down' }
  | { kind: 'travel'; to_x: number; to_y: number };

export interface SheetCutPlan2D {
  sheet_name: string;
  sheet_index_in_solution: number;
  total_cost: number;
  num_cuts: number;
  num_rotations: number;
  num_fence_resets: number;
  num_tool_ups: number;
  travel_distance: number;
  steps: CutStep2D[];
}

export interface CutPlanSolution2D {
  preset: CutPlanPreset2D;
  effective_costs: EffectiveCosts2D;
  sheet_plans: SheetCutPlan2D[];
  total_cost: number;
}

// ---------------------------------------------------------------------------
// Exported functions
// ---------------------------------------------------------------------------

/**
 * Solve a 1D cutting-stock problem. Accepts a plain JS object and returns the
 * solution as a plain JS object. Throws on validation errors, infeasible
 * demands, or unsupported solver configurations.
 */
export function solve1d(
  problem: OneDProblem,
  options?: OneDOptions,
): OneDSolution;

/**
 * Solve a 2D rectangular bin-packing problem. Accepts a plain JS object and
 * returns the solution as a plain JS object. Throws on validation errors,
 * infeasible demands, or unsupported solver configurations.
 */
export function solve2d(
  problem: TwoDProblem,
  options?: TwoDOptions,
): TwoDSolution;

/**
 * JSON-string variant of `solve1d`. Accepts a JSON-encoded problem and an
 * optional JSON-encoded options string. Returns a JSON-encoded solution.
 */
export function solve1dJson(
  problem_json: string,
  options_json?: string | null,
): string;

/**
 * JSON-string variant of `solve2d`.
 */
export function solve2dJson(
  problem_json: string,
  options_json?: string | null,
): string;

/**
 * Solve a 3D rectangular bin-packing problem. Accepts a plain JS object and
 * returns the solution as a plain JS object. Throws on validation errors,
 * infeasible demands, or unsupported solver configurations.
 */
export function solve3d(
  problem: ThreeDProblem,
  options?: ThreeDOptions,
): ThreeDSolution;

/**
 * JSON-string variant of `solve3d`.
 */
export function solve3dJson(
  problem_json: string,
  options_json?: string | null,
): string;

/**
 * Generate a cut plan for a finished 1D solution. Accepts plain JS objects
 * matching the `OneDProblem` and `OneDSolution` shapes, and an optional
 * options object. Returns the cut plan as a plain JS object.
 */
export function plan1dCuts(
  problem: OneDProblem,
  solution: OneDSolution,
  options?: CutPlanOptions1D,
): CutPlanSolution1D;

/**
 * Generate a cut plan for a finished 2D solution. Accepts a plain JS object
 * matching the `TwoDSolution` shape and an optional options object. Returns
 * the cut plan as a plain JS object.
 */
export function plan2dCuts(
  solution: TwoDSolution,
  options?: CutPlanOptions2D,
): CutPlanSolution2D;

/** Return the package version string. */
export function version(): string;
