export interface Stock1D {
  name: string
  length: number
  kerf?: number
  trim?: number
  cost?: number
  available?: number | null
}

export interface CutDemand1D {
  name: string
  length: number
  quantity: number
}

export interface OneDProblem {
  stock: Stock1D[]
  demands: CutDemand1D[]
}

export interface OneDOptions {
  algorithm?: 'auto' | 'first_fit_decreasing' | 'best_fit_decreasing' | 'local_search' | 'column_generation'
  multistart_runs?: number
  improvement_rounds?: number
  column_generation_rounds?: number
  exact_pattern_limit?: number
  auto_exact_max_types?: number
  auto_exact_max_quantity?: number
  seed?: number | null
}

export interface CutAssignment1D {
  name: string
  length: number
}

export interface StockLayout1D {
  stock_name: string
  stock_length: number
  used_length: number
  remaining_length: number
  waste: number
  cost: number
  cuts: CutAssignment1D[]
}

export interface StockRequirement1D {
  stock_name: string
  stock_length: number
  usable_length: number
  cost: number
  available_quantity: number | null
  used_quantity: number
  required_quantity: number
  additional_quantity_needed: number
}

export interface SolverMetrics1D {
  iterations: number
  generated_patterns: number
  enumerated_patterns: number
  explored_states: number
  notes: string[]
}

export interface OneDSolution {
  algorithm: string
  exact: boolean
  lower_bound: number | null
  stock_count: number
  total_waste: number
  total_cost: number
  layouts: StockLayout1D[]
  stock_requirements: StockRequirement1D[]
  unplaced: CutAssignment1D[]
  metrics: SolverMetrics1D
}

export interface Sheet2D {
  name: string
  width: number
  height: number
  cost?: number
  quantity?: number | null
  kerf?: number
  /** When true, the trailing placement may extend up to one kerf past
   *  the sheet's right and bottom edges, modeling a cut that runs off
   *  the stock. Does not relax individual part size limits. Default: false. */
  edge_kerf_relief?: boolean
}

export interface RectDemand2D {
  name: string
  width: number
  height: number
  quantity: number
  can_rotate?: boolean
}

export interface TwoDProblem {
  sheets: Sheet2D[]
  demands: RectDemand2D[]
}

export interface TwoDOptions {
  algorithm?: 'auto' | 'max_rects' | 'max_rects_best_short_side_fit' | 'max_rects_best_long_side_fit' | 'max_rects_bottom_left' | 'max_rects_contact_point' | 'skyline' | 'skyline_min_waste' | 'guillotine' | 'guillotine_best_short_side_fit' | 'guillotine_best_long_side_fit' | 'guillotine_shorter_leftover_axis' | 'guillotine_longer_leftover_axis' | 'guillotine_min_area_split' | 'guillotine_max_area_split' | 'next_fit_decreasing_height' | 'first_fit_decreasing_height' | 'best_fit_decreasing_height' | 'multi_start' | 'rotation_search'
  multistart_runs?: number
  beam_width?: number
  guillotine_required?: boolean
  min_usable_side?: number
  auto_rotation_search_max_types?: number
  seed?: number | null
}

export interface Placement2D {
  name: string
  x: number
  y: number
  width: number
  height: number
  rotated: boolean
}

export interface SheetLayout2D {
  sheet_name: string
  width: number
  height: number
  cost: number
  placements: Placement2D[]
  used_area: number
  waste_area: number
  kerf_area: number
  largest_usable_drop_area: number
  sum_sq_usable_drop_areas: number
}

export interface SolverMetrics2D {
  iterations: number
  explored_states: number
  notes: string[]
}

export interface TwoDSolution {
  algorithm: string
  guillotine: boolean
  sheet_count: number
  total_waste_area: number
  total_kerf_area: number
  total_cost: number
  max_usable_drop_area: number
  total_sum_sq_usable_drop_areas: number
  layouts: SheetLayout2D[]
  unplaced: RectDemand2D[]
  metrics: SolverMetrics2D
}

export interface Bin3D {
  name: string
  width: number
  height: number
  depth: number
  cost?: number
  quantity?: number | null
}

export interface BoxDemand3D {
  name: string
  width: number
  height: number
  depth: number
  quantity: number
  allowed_rotations?: number
}

export interface ThreeDProblem {
  bins: Bin3D[]
  demands: BoxDemand3D[]
}

export interface ThreeDOptions {
  algorithm?: 'auto' | 'extreme_points' | 'extreme_points_residual_space' | 'extreme_points_free_volume' | 'extreme_points_bottom_left_back' | 'extreme_points_contact_point' | 'extreme_points_euclidean' | 'guillotine_3d' | 'guillotine_3d_best_short_side_fit' | 'guillotine_3d_best_long_side_fit' | 'guillotine_3d_shorter_leftover_axis' | 'guillotine_3d_longer_leftover_axis' | 'guillotine_3d_min_volume_split' | 'guillotine_3d_max_volume_split' | 'layer_building' | 'layer_building_max_rects' | 'layer_building_skyline' | 'layer_building_guillotine' | 'layer_building_shelf' | 'wall_building' | 'column_building' | 'deepest_bottom_left' | 'deepest_bottom_left_fill' | 'first_fit_decreasing_volume' | 'best_fit_decreasing_volume' | 'multi_start' | 'grasp' | 'local_search' | 'branch_and_bound'
  multistart_runs?: number
  improvement_rounds?: number
  beam_width?: number
  seed?: number | null
  branch_and_bound_node_limit?: number
  guillotine_required?: boolean
  auto_exact_max_types?: number
  auto_exact_max_quantity?: number
}

export interface Placement3D {
  name: string
  x: number
  y: number
  z: number
  width: number
  height: number
  depth: number
  rotation: 'xyz' | 'xzy' | 'yxz' | 'yzx' | 'zxy' | 'zyx'
}

export interface BinLayout3D {
  bin_name: string
  width: number
  height: number
  depth: number
  cost: number
  placements: Placement3D[]
  used_volume: number
  waste_volume: number
}

export interface BinRequirement3D {
  bin_name: string
  bin_width: number
  bin_height: number
  bin_depth: number
  cost: number
  available_quantity: number | null
  used_quantity: number
  required_quantity: number
  additional_quantity_needed: number
}

export interface SolverMetrics3D {
  iterations: number
  explored_states: number
  extreme_points_generated: number
  branch_and_bound_nodes: number
  notes: string[]
}

export interface ThreeDSolution {
  algorithm: string
  exact: boolean
  lower_bound: number | null
  guillotine: boolean
  bin_count: number
  total_waste_volume: number
  total_cost: number
  layouts: BinLayout3D[]
  bin_requirements: BinRequirement3D[]
  unplaced: BoxDemand3D[]
  metrics: SolverMetrics3D
}

// ---------------------------------------------------------------------------
// Cut planning
// ---------------------------------------------------------------------------

export type CutPlanPreset1D = 'chop_saw'

export interface CutPlanOptions1D {
  preset?: CutPlanPreset1D
  cut_cost?: number
  fence_reset_cost?: number
}

export interface EffectiveCosts1D {
  cut_cost: number
  fence_reset_cost: number
}

export type CutStep1D =
  | { kind: 'cut'; position: number; piece_name: string }
  | { kind: 'fence_reset'; new_position: number }

export interface BarCutPlan1D {
  stock_name: string
  bar_index_in_solution: number
  total_cost: number
  num_cuts: number
  num_fence_resets: number
  steps: CutStep1D[]
}

export interface CutPlanSolution1D {
  preset: CutPlanPreset1D
  effective_costs: EffectiveCosts1D
  bar_plans: BarCutPlan1D[]
  total_cost: number
}

export type CutPlanPreset2D = 'table_saw' | 'panel_saw' | 'cnc_router'

export interface CutPlanOptions2D {
  preset?: CutPlanPreset2D
  cut_cost?: number
  rotate_cost?: number
  fence_reset_cost?: number
  tool_up_down_cost?: number
  travel_cost?: number
}

export interface EffectiveCosts2D {
  cut_cost: number
  rotate_cost: number
  fence_reset_cost: number
  tool_up_down_cost: number
  travel_cost: number
}

export type CutAxis = 'vertical' | 'horizontal'

export type CutStep2D =
  | { kind: 'cut'; axis: CutAxis; position: number }
  | { kind: 'line_cut'; from_x: number; from_y: number; to_x: number; to_y: number }
  | { kind: 'rotate' }
  | { kind: 'fence_reset'; new_position: number }
  | { kind: 'tool_up' }
  | { kind: 'tool_down' }
  | { kind: 'travel'; to_x: number; to_y: number }

export interface SheetCutPlan2D {
  sheet_name: string
  sheet_index_in_solution: number
  total_cost: number
  num_cuts: number
  num_rotations: number
  num_fence_resets: number
  num_tool_ups: number
  travel_distance: number
  steps: CutStep2D[]
}

export interface CutPlanSolution2D {
  preset: CutPlanPreset2D
  effective_costs: EffectiveCosts2D
  sheet_plans: SheetCutPlan2D[]
  total_cost: number
}

export declare function solve1d(problem: OneDProblem, options?: OneDOptions): OneDSolution
export declare function solve2d(problem: TwoDProblem, options?: TwoDOptions): TwoDSolution
export declare function solve3d(problem: ThreeDProblem, options?: ThreeDOptions): ThreeDSolution
export declare function solve1D(problem: OneDProblem, options?: OneDOptions): OneDSolution
export declare function solve2D(problem: TwoDProblem, options?: TwoDOptions): TwoDSolution
export declare function solve3D(problem: ThreeDProblem, options?: ThreeDOptions): ThreeDSolution
export declare function plan1dCuts(problem: OneDProblem, solution: OneDSolution, options?: CutPlanOptions1D): CutPlanSolution1D
export declare function plan2dCuts(solution: TwoDSolution, options?: CutPlanOptions2D): CutPlanSolution2D
export declare function plan1d_cuts(problem: OneDProblem, solution: OneDSolution, options?: CutPlanOptions1D): CutPlanSolution1D
export declare function plan2d_cuts(solution: TwoDSolution, options?: CutPlanOptions2D): CutPlanSolution2D
export declare function version(): string

// Workpiece-aware saw planner, schema version 1.
/** Geometry only; packing-result layouts are structurally compatible. */
export interface SawLayout2D {
  width: number;
  height: number;
  placements: Placement2D[];
}
/** Explicit source indices are required; names are labels, never identity keys. */
export interface SawJob2D {
  problem: TwoDProblem;
  layouts: SawLayout2D[];
  stock_indices: number[];
  demand_indices: number[][];
}
export type SawAxis2D = 'vertical' | 'horizontal';
export type ReferenceEdge2D = 'left' | 'top' | 'right' | 'bottom';
export type SawMachine2D = 'table_saw' | 'panel_saw';
export type SawSetupKind2D = 'rip_fence' | 'crosscut_stop' | 'panel_stop';
export type SawObjective2D = 'fewest_setup_changes' | 'fewest_cuts' | 'weighted_effort';
export interface SawSetup2D { kind: SawSetupKind2D; distance: number }
/** Effort units, not time or material price. All weights must be finite, 0 through 1e100. */
export interface SawWeights2D {
  cut: number;
  initial_setup: number;
  setup_change: number;
  rotation: number;
  handling: number;
}
export interface SawSearchLimits2D {
  max_states: number;
  beam_width: number;
  baseline_max_states: number;
}
export interface SawPlanOptions2D {
  machine?: SawMachine2D;
  objective?: SawObjective2D;
  weights?: Partial<SawWeights2D>;
  batch_across_sheets?: boolean;
  search?: Partial<SawSearchLimits2D>;
  min_usable_side?: number;
}
export interface SawRect2D { x: number; y: number; width: number; height: number }
export interface WorkpieceId2D { sheet_index: number; bounds: SawRect2D }
export interface PartId2D { sheet_index: number; placement_index: number }
export interface SawWorkpiece2D {
  id: WorkpieceId2D;
  parent: WorkpieceId2D | null;
  created_by: number | null;
  finished_part: PartId2D | null;
  usable_offcut: boolean;
}
export interface SawSegment2D { from_x: number; from_y: number; to_x: number; to_y: number }
export type SawAction2D =
  | { kind: 'handle' }
  | { kind: 'rotate'; quarter_turns: number }
  | { kind: 'setup'; setup: SawSetup2D; initial: boolean }
  | { kind: 'cut'; axis: SawAxis2D; reference_edge: ReferenceEdge2D; setting: number;
      kerf: number; kerf_region: SawRect2D; segment: SawSegment2D; children: WorkpieceId2D[] };
export interface SawOperation2D {
  id: number;
  workpiece: WorkpieceId2D;
  depends_on: number | null;
  action: SawAction2D;
}
export interface SawCounts2D {
  cuts: number;
  initial_setups: number;
  setup_changes: number;
  rotations: number;
  handling: number;
}
export type SawOptimality2D = 'heuristic' | 'optimal_within_candidate_model';
export interface SawSearchReport2D {
  limits: SawSearchLimits2D;
  baseline_states: number;
  explored_states: number;
  budget_exhausted: boolean;
  beam_pruned: boolean;
  /** Equivalent future states rejected by operation-count dominance. */
  dominated_states: number;
  /** Peak dominance storage in scalar units, at most 65,536. */
  peak_dominance_units: number;
  /** Some moves bypassed a full cache without being discarded. */
  dominance_table_saturated: boolean;
  optimality: SawOptimality2D;
  baseline_effort_score: number;
  baseline_counts: SawCounts2D;
}
/** Diagram references, not an independently executable per-sheet plan. */
export interface SawSheetView2D {
  sheet_index_in_solution: number;
  stock_index: number;
  operation_ids: number[];
}
export interface SawPlan2D {
  schema_version: number;
  machine: SawMachine2D;
  objective: SawObjective2D;
  batch_across_sheets: boolean;
  operations: SawOperation2D[];
  workpieces: SawWorkpiece2D[];
  sheet_views: SawSheetView2D[];
  counts: SawCounts2D;
  effective_weights: SawWeights2D;
  effort_score: number;
  search: SawSearchReport2D;
}
/** Plan a physical saw job. Execute operations in order; sheet views are diagram references. */
export function plan2dSawCuts(job: SawJob2D, options?: SawPlanOptions2D | null): SawPlan2D;

/** Integer constraints in the sheet's original coordinate frame. */
export interface Defect2D {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface SheetConstraints2D {
  trim_left?: number;
  trim_right?: number;
  trim_top?: number;
  trim_bottom?: number;
  defects?: Defect2D[];
}
export interface ConstrainedProblem2D {
  problem: TwoDProblem;
  constraints?: SheetConstraints2D[];
}
export type PackingObjective2D = 'bin_count' | 'total_cost';
export interface ConstrainedOptions2D {
  objective?: PackingObjective2D;
  beam_width?: number;
  max_states?: number;
}
export interface ConstrainedSolution2D {
  solution: TwoDSolution;
  stock_indices: number[];
  demand_indices: number[][];
  explored_states: number;
  search_truncated: boolean;
}
/** Trims/defects, finite inventory and count-first or cost-first construction. */
export function solve2dConstrained(problem: ConstrainedProblem2D, options?: ConstrainedOptions2D | null): ConstrainedSolution2D;
export interface AdaptiveOptions2D {
  solver?: TwoDOptions;
  max_candidates?: number;
  stop_on_count_bound?: boolean;
}
export interface AdaptiveSolution2D {
  solution: TwoDSolution;
  lower_bound: number;
  count_optimal: boolean;
  candidates_run: number;
  stopped_on_bound: boolean;
  candidate_limit_reached: boolean;
}
/** A count certificate does not certify offcuts or other secondary metrics. */
export function solve2dAdaptive(problem: TwoDProblem, options?: AdaptiveOptions2D | null): AdaptiveSolution2D;
/** Conservative sheet-count bound, relaxing inventory and blade gaps. */
export function lowerBound2d(problem: TwoDProblem): number;

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
/** Bounded cost-first search; exact certifies the selected material-cost objective. */
export function solve1dMinCost(problem: OneDProblem, options?: OneDOptions | null): OneDSolution;

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

export type OptimizationProblem1D = OptimizationProblem<OneDProblem>;
export type OptimizedSolution1D = OptimizedSolution<OneDSolution, OneDProblem>;
/** Optimize profit, material, feasibility or open X with source provenance. */
export function solve1dOptimized(problem: OptimizationProblem1D, options?: OptimizationOptions | null): OptimizedSolution1D;

export type OptimizationProblem2D = OptimizationProblem<ConstrainedProblem2D>;
export type OptimizedSolution2D = OptimizedSolution<TwoDSolution, ConstrainedProblem2D>;
export function solve2dOptimized(problem: OptimizationProblem2D, options?: OptimizationOptions | null): OptimizedSolution2D;
export type StagedAxis2D = 'vertical' | 'horizontal';
export type FirstStageAxis2D = 'any' | StagedAxis2D;
export type StagedCutType2D = 'exact' | 'non_exact' | 'homogeneous' | 'roadef2018';
export interface StagedRules2D {
  max_stages?: number | null;
  first_axis?: FirstStageAxis2D;
  cut_type?: StagedCutType2D;
  minimum_distance_1_cuts?: number;
  maximum_distance_1_cuts?: number | null;
  minimum_distance_2_cuts?: number;
  maximum_distance_2_cuts?: number | null;
  maximum_number_1_cuts?: number | null;
  maximum_number_2_cuts?: number | null;
  minimum_waste_length?: number;
  cut_through_defects?: boolean;
}
export interface StagedProblem2D {
  packing: ConstrainedProblem2D;
  rules?: StagedRules2D;
  groups?: (number | null)[];
}
export interface StagedOptions2D {
  objective?: PackingObjective2D;
  beam_width?: number;
  max_states?: number;
}
export interface StagedRegion2D { x: number; y: number; width: number; height: number }
export type StagedAction2D =
  | { kind: 'cut'; axis: StagedAxis2D; offset: number; children: number[] }
  | { kind: 'part'; demand_index: number; rotated: boolean }
  | { kind: 'waste' };
export interface StagedNode2D { region: StagedRegion2D; action: StagedAction2D }
/** Prepared trimmed stock starts at node zero; all finishing cuts remain explicit. */
export interface StagedTree2D { first_axis: StagedAxis2D; nodes: StagedNode2D[] }
export interface StagedPart2D { layout_index: number; node_index: number }
export interface StagedSolution2D {
  solution: TwoDSolution;
  stock_indices: number[];
  demand_indices: number[][];
  unplaced_quantities: number[];
  trees: StagedTree2D[];
  production_order: StagedPart2D[];
  explored_states: number;
  search_truncated: boolean;
}
export type OptimizationProblemStaged2D = OptimizationProblem<StagedProblem2D>;
export type OptimizedSolutionStaged2D = OptimizedSolution<StagedSolution2D, StagedProblem2D>;
/** Construct and independently validate a bounded staged guillotine cut tree. */
export function solve2dStaged(problem: StagedProblem2D, options?: StagedOptions2D | null): StagedSolution2D;
export function solve2dStagedOptimized(problem: OptimizationProblemStaged2D, options?: OptimizationOptions | null): OptimizedSolutionStaged2D;

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

/** Finite coordinates in the original source shape's common local frame. */
export interface Point2D { x: number; y: number }
export type CurveSegment2D =
  | { kind: 'line'; end: Point2D }
  | { kind: 'arc'; end: Point2D; center: Point2D; clockwise: boolean };
export type Contour2D =
  | { kind: 'polygon'; vertices: Point2D[] }
  | { kind: 'circle'; center: Point2D; radius: number }
  | { kind: 'curve'; start: Point2D; segments: CurveSegment2D[] };
export interface Polygon2D { exterior: Contour2D; holes?: Contour2D[] }
export interface Shape2D { components: Polygon2D[] }
export interface RotationRange2D { start: number; end: number }
export interface IrregularBin {
  name: string;
  shape: Shape2D;
  cost?: number;
  quantity?: number | null;
  defects?: Shape2D[];
  spacing?: number;
}
export interface IrregularDemand {
  name: string;
  shape: Shape2D;
  quantity: number;
  allowed_rotations?: RotationRange2D[];
  can_mirror?: boolean;
  profit?: number;
}
export interface IrregularProblem { bins: IrregularBin[]; demands: IrregularDemand[] }
export type IrregularObjective = 'bin_count' | 'total_cost' | 'knapsack' | 'feasibility'
  | 'open_dimension_x' | 'open_dimension_y' | 'open_dimension_xy';
export interface IrregularOptions {
  objective?: IrregularObjective;
  max_states?: number;
  beam_width?: number;
  max_candidates_per_item?: number;
  rotation_step_degrees?: number;
  max_orientations?: number;
  curve_tolerance?: number;
  item_spacing?: number;
  bin_spacing?: number;
  defect_spacing?: number;
}
export interface IrregularPlacement {
  demand_index: number;
  x: number;
  y: number;
  rotation_degrees: number;
  mirrored: boolean;
}
export interface IrregularLayout {
  bin_index: number;
  placements: IrregularPlacement[];
  used_area: number;
  used_width: number;
  used_height: number;
}
/** Returned controls are fully populated even when options were omitted. */
export interface IrregularSolution {
  algorithm: string;
  options: Required<IrregularOptions>;
  layouts: IrregularLayout[];
  unplaced_quantities: number[];
  total_cost: number;
  total_profit: number;
  feasible: boolean;
  explored_states: number;
  construction_probes: number;
  search_truncated: boolean;
}
export function solveIrregular(problem: IrregularProblem, options?: IrregularOptions | null): IrregularSolution;
