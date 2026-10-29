// Native irregular-only WASM object API. All geometry numbers must be finite.

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

export function version(): string;
