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

export interface ConstrainedProblem3D {
  problem: ThreeDProblem;
  /** Nonnegative safe integers in one consistent weight unit. */
  item_weights?: number[];
  /** Payload capacity, excluding carton tare weight; null means unlimited. */
  bin_max_weights?: (number | null)[];
}
export type PackingObjective3D = 'bin_count' | 'total_cost';
export interface ConstrainedOptions3D {
  objective?: PackingObjective3D;
  beam_width?: number;
  max_states?: number;
}
export interface ConstrainedSolution3D {
  solution: ThreeDSolution;
  bin_indices: number[];
  demand_indices: number[][];
  bin_weights: number[];
  explored_states: number;
  search_truncated: boolean;
}
/** Weight-aware placement with count-first or cost-first construction.
 * Input weights and output bin weights must be JavaScript safe integers. */
export function solve3dConstrained(problem: ConstrainedProblem3D, options?: ConstrainedOptions3D | null): ConstrainedSolution3D;

/** Native explicit objectives. Bounds certify only the primary objective. */
export type PackingObjective = 'bin_count' | 'total_cost' | 'knapsack' | 'feasibility'
  | 'open_dimension_x' | 'open_dimension_y' | 'open_dimension_z' | 'open_dimension_xy';
export interface OptimizationProblem<P> {
  packing: P;
  /** Finite nonnegative value per source demand; empty defaults to one. */
  profits?: number[];
  /** Mandatory source copies under knapsack; empty defaults to zero. */
  minimum_quantities?: number[];
}
export interface OptimizationOptions {
  objective?: PackingObjective;
  max_evaluations?: number;
  packing_max_states?: number;
  beam_width?: number;
  /** Nonnegative JavaScript safe integer. */
  seed?: number;
}
export interface OptimizationReport {
  objective: PackingObjective;
  complete: boolean;
  minimums_satisfied: boolean;
  selected_quantities: number[];
  total_profit: number;
  evaluations: number;
  packing_states: number;
  search_truncated: boolean;
  open_extents: number[];
  primary_value: number;
  primary_bound: number | null;
  bound_is_upper: boolean;
  primary_optimal: boolean;
}
export interface OptimizedSolution<S, P> {
  effective_problem: P;
  solution: S;
  stock_indices: number[];
  demand_indices: number[][];
  report: OptimizationReport;
}

export type OptimizationProblem3D = OptimizationProblem<ConstrainedProblem3D>;
export type OptimizedSolution3D = OptimizedSolution<ThreeDSolution, ConstrainedProblem3D>;
export function solve3dOptimized(problem: OptimizationProblem3D, options?: OptimizationOptions | null): OptimizedSolution3D;
export type FloorAxis3D = 'x' | 'z';
export type UnloadingConstraint3D = 'none' | 'only_x_movements' | 'only_z_movements' | 'increasing_x' | 'increasing_z';
/** Every weight and loading limit is a nonnegative JavaScript safe integer. */
export interface StackItemRules3D {
  stackability_id?: number;
  nesting_height?: number;
  maximum_stackability?: number | null;
  maximum_weight_above?: number | null;
  group_id?: number;
}
export interface WeightDensity3D { numerator: number; denominator: number }
export interface FloorDefect3D { x: number; z: number; width: number; depth: number }
export interface AxleLimits3D {
  axis: FloorAxis3D;
  front_position: number;
  rear_position: number;
  front_maximum: number;
  rear_maximum: number;
  front_tare?: number;
  rear_tare?: number;
}
export interface LoadingBinRules3D {
  maximum_stack_density?: WeightDensity3D | null;
  axle_limits?: AxleLimits3D | null;
  defects?: FloorDefect3D[];
}
export interface LoadingProblem3D {
  packing: ConstrainedProblem3D;
  item_rules?: StackItemRules3D[];
  bin_rules?: LoadingBinRules3D[];
  unloading?: UnloadingConstraint3D;
}
export interface LoadingOptions3D {
  objective?: PackingObjective3D;
  beam_width?: number;
  max_states?: number;
}
export interface StackedItem3D {
  demand_index: number;
  y: number;
  rotation: 'xyz' | 'xzy' | 'yxz' | 'yzx' | 'zxy' | 'zyx';
}
export interface StackColumn3D {
  x: number;
  z: number;
  width: number;
  depth: number;
  height: number;
  weight: number;
  items: StackedItem3D[];
}
export interface AxleLoads3D { front: number; rear: number }
export interface LoadingBinLayout3D {
  bin_index: number;
  payload: number;
  stacks: StackColumn3D[];
  axle_loads: AxleLoads3D | null;
}
export interface UnplacedLoadingItem3D { demand_index: number; quantity: number }
export interface LoadingSolution3D {
  layouts: LoadingBinLayout3D[];
  unplaced: UnplacedLoadingItem3D[];
  bin_count: number;
  total_cost: number;
  explored_states: number;
  search_truncated: boolean;
}
export type OptimizationProblemLoading3D = OptimizationProblem<LoadingProblem3D>;
export type OptimizedSolutionLoading3D = OptimizedSolution<LoadingSolution3D, LoadingProblem3D>;
/** Same-footprint nested stacks, payload, density, axles and unloading rules. */
export function solve3dLoading(problem: LoadingProblem3D, options?: LoadingOptions3D | null): LoadingSolution3D;
export function solve3dLoadingOptimized(problem: OptimizationProblemLoading3D, options?: OptimizationOptions | null): OptimizedSolutionLoading3D;
