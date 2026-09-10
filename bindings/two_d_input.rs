//! Decode only geometry consumed by legacy cut planning. Large derived u128
//! metrics become exponent-form JS numbers and must not be deserialized again.
use bin_packing::two_d::{Placement2D, SheetLayout2D, SolverMetrics2D, TwoDSolution};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct CutSolutionInput {
    layouts: Vec<CutLayoutInput>,
}

#[derive(Deserialize)]
struct CutLayoutInput {
    sheet_name: String,
    width: u32,
    height: u32,
    placements: Vec<Placement2D>,
}

impl CutSolutionInput {
    pub(crate) fn into_solution(self) -> TwoDSolution {
        TwoDSolution {
            algorithm: String::new(),
            guillotine: false,
            sheet_count: self.layouts.len(),
            total_waste_area: 0,
            total_kerf_area: 0,
            total_cost: 0.0,
            max_usable_drop_area: 0,
            total_sum_sq_usable_drop_areas: 0,
            unplaced: Vec::new(),
            metrics: SolverMetrics2D { iterations: 0, explored_states: 0, notes: Vec::new() },
            layouts: self
                .layouts
                .into_iter()
                .map(|layout| SheetLayout2D {
                    sheet_name: layout.sheet_name,
                    width: layout.width,
                    height: layout.height,
                    placements: layout.placements,
                    cost: 0.0,
                    used_area: 0,
                    waste_area: 0,
                    kerf_area: 0,
                    largest_usable_drop_area: 0,
                    sum_sq_usable_drop_areas: 0,
                })
                .collect(),
        }
    }
}
