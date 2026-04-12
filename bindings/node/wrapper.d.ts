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
  algorithm?: 'auto' | 'max_rects' | 'max_rects_best_short_side_fit' | 'max_rects_best_long_side_fit' | 'max_rects_bottom_left' | 'max_rects_contact_point' | 'skyline' | 'skyline_min_waste' | 'guillotine' | 'guillotine_best_short_side_fit' | 'guillotine_best_long_side_fit' | 'guillotine_shorter_leftover_axis' | 'guillotine_longer_leftover_axis' | 'guillotine_min_area_split' | 'guillotine_max_area_split' | 'next_fit_decreasing_height' | 'first_fit_decreasing_height' | 'best_fit_decreasing_height' | 'multi_start'
  multistart_runs?: number
  beam_width?: number
  guillotine_required?: boolean
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
  total_cost: number
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

export declare function solve1d(problem: OneDProblem, options?: OneDOptions): OneDSolution
export declare function solve2d(problem: TwoDProblem, options?: TwoDOptions): TwoDSolution
export declare function solve3d(problem: ThreeDProblem, options?: ThreeDOptions): ThreeDSolution
export declare function solve1D(problem: OneDProblem, options?: OneDOptions): OneDSolution
export declare function solve2D(problem: TwoDProblem, options?: TwoDOptions): TwoDSolution
export declare function solve3D(problem: ThreeDProblem, options?: ThreeDOptions): ThreeDSolution
export declare function version(): string
