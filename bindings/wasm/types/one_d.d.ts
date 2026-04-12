/* tslint:disable */
/* eslint-disable */
//
// Hand-written TypeScript definitions for the 1D-only WASM entry points.

/** Algorithm selector for `solve1d`. */
export type OneDAlgorithm =
  | 'auto'
  | 'first_fit_decreasing'
  | 'best_fit_decreasing'
  | 'local_search'
  | 'column_generation';

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

/**
 * Solve a 1D cutting-stock problem. Accepts a plain JS object and returns the
 * solution as a plain JS object. Throws on validation errors, infeasible
 * demands, or unsupported solver configurations.
 */
export function solve1d(
  problem: OneDProblem,
  options?: OneDOptions,
): OneDSolution;

/** Return the package version string. */
export function version(): string;
