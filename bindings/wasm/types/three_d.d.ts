/* tslint:disable */
/* eslint-disable */
//
// Hand-written TypeScript definitions for the 3D-only WASM entry points.

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

/**
 * Solve a 3D rectangular bin-packing problem. Accepts a plain JS object and
 * returns the solution as a plain JS object. Throws on validation errors,
 * infeasible demands, or unsupported solver configurations.
 */
export function solve3d(
  problem: ThreeDProblem,
  options?: ThreeDOptions,
): ThreeDSolution;

/** Return the package version string. */
export function version(): string;
