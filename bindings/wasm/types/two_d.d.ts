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
