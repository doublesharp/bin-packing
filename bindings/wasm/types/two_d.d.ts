/* tslint:disable */
/* eslint-disable */
//
// Hand-written TypeScript definitions for the 2D-only WASM entry points.

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
