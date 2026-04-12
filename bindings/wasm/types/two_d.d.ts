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
  | 'multi_start';

export interface Sheet2D {
  name: string;
  width: number;
  height: number;
  cost?: number;
  quantity?: number | null;
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
  total_cost: number;
  layouts: SheetLayout2D[];
  unplaced: RectDemand2D[];
  metrics: SolverMetrics2D;
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

/** Return the package version string. */
export function version(): string;
