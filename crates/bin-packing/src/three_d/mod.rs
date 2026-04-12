//! Three-dimensional rectangular bin packing solvers.
//!
//! Provides Extreme Points (six scoring variants), Guillotine 3D beam search (seven
//! variants), horizontal layer-building (five inner-2D backends), Bischoff & Marriott
//! vertical wall-building, column / stack building, Deepest-Bottom-Left and DBLF,
//! volume-sorted FFD/BFD heuristics, Multi-start, GRASP, and Local Search
//! meta-strategies, plus a restricted Martello-Pisinger-Vigo branch-and-bound exact
//! backend.

mod auto;
mod column;
mod common;
mod dblf;
mod exact;
mod extreme_points;
mod grasp;
mod guillotine;
mod layer;
mod local_search;
mod model;
mod multi_start;
mod sorted;
mod wall;

pub use model::{
    Bin3D, BinLayout3D, BinRequirement3D, BoxDemand3D, MAX_BIN_COUNT_3D, MAX_DIMENSION_3D,
    Placement3D, Rotation3D, RotationMask3D, SolverMetrics3D, ThreeDAlgorithm, ThreeDOptions,
    ThreeDProblem, ThreeDSolution,
};

use crate::Result;

/// Solve a 3D rectangular bin packing problem using the requested algorithm.
///
/// Validates the problem and dispatches to the algorithm selected in `options`. Use
/// [`ThreeDAlgorithm::Auto`] to let the solver try several strategies and return the
/// best result.
///
/// # Errors
///
/// Returns [`BinPackingError::InvalidInput`](crate::BinPackingError::InvalidInput) for
/// malformed problems and
/// [`BinPackingError::Infeasible3D`](crate::BinPackingError::Infeasible3D) when at
/// least one demand cannot fit any declared bin in any allowed rotation.
pub fn solve_3d(problem: ThreeDProblem, options: ThreeDOptions) -> Result<ThreeDSolution> {
    problem.validate()?;
    problem.ensure_feasible_demands()?;
    let mut solution = solve_3d_core(&problem, &options)?;

    if problem.bins.iter().any(|bin| bin.quantity.is_some()) {
        let required_counts = estimate_required_bin_counts(&problem, &options)?;
        set_bin_requirements(&mut solution, &problem.bins, &required_counts);
        solution
            .metrics
            .notes
            .push("bin requirements estimated from a relaxed-inventory auto solve".to_string());
    }

    Ok(solution)
}

fn solve_3d_core(problem: &ThreeDProblem, options: &ThreeDOptions) -> Result<ThreeDSolution> {
    match options.algorithm {
        ThreeDAlgorithm::ExtremePoints => extreme_points::solve_extreme_points(problem, options),
        ThreeDAlgorithm::ExtremePointsResidualSpace => {
            extreme_points::solve_extreme_points_residual_space(problem, options)
        }
        ThreeDAlgorithm::ExtremePointsFreeVolume => {
            extreme_points::solve_extreme_points_free_volume(problem, options)
        }
        ThreeDAlgorithm::ExtremePointsBottomLeftBack => {
            extreme_points::solve_extreme_points_bottom_left_back(problem, options)
        }
        ThreeDAlgorithm::ExtremePointsContactPoint => {
            extreme_points::solve_extreme_points_contact_point(problem, options)
        }
        ThreeDAlgorithm::ExtremePointsEuclidean => {
            extreme_points::solve_extreme_points_euclidean(problem, options)
        }
        ThreeDAlgorithm::Guillotine3D => guillotine::solve_guillotine_3d(problem, options),
        ThreeDAlgorithm::Guillotine3DBestShortSideFit => {
            guillotine::solve_guillotine_3d_best_short_side_fit(problem, options)
        }
        ThreeDAlgorithm::Guillotine3DBestLongSideFit => {
            guillotine::solve_guillotine_3d_best_long_side_fit(problem, options)
        }
        ThreeDAlgorithm::Guillotine3DShorterLeftoverAxis => {
            guillotine::solve_guillotine_3d_shorter_leftover_axis(problem, options)
        }
        ThreeDAlgorithm::Guillotine3DLongerLeftoverAxis => {
            guillotine::solve_guillotine_3d_longer_leftover_axis(problem, options)
        }
        ThreeDAlgorithm::Guillotine3DMinVolumeSplit => {
            guillotine::solve_guillotine_3d_min_volume_split(problem, options)
        }
        ThreeDAlgorithm::Guillotine3DMaxVolumeSplit => {
            guillotine::solve_guillotine_3d_max_volume_split(problem, options)
        }
        ThreeDAlgorithm::LayerBuilding => layer::solve_layer_building(problem, options),
        ThreeDAlgorithm::LayerBuildingMaxRects => {
            layer::solve_layer_building_max_rects(problem, options)
        }
        ThreeDAlgorithm::LayerBuildingSkyline => {
            layer::solve_layer_building_skyline(problem, options)
        }
        ThreeDAlgorithm::LayerBuildingGuillotine => {
            layer::solve_layer_building_guillotine(problem, options)
        }
        ThreeDAlgorithm::LayerBuildingShelf => layer::solve_layer_building_shelf(problem, options),
        ThreeDAlgorithm::DeepestBottomLeft => dblf::solve_deepest_bottom_left(problem, options),
        ThreeDAlgorithm::DeepestBottomLeftFill => {
            dblf::solve_deepest_bottom_left_fill(problem, options)
        }
        ThreeDAlgorithm::BranchAndBound => exact::solve_branch_and_bound(problem, options),
        ThreeDAlgorithm::WallBuilding => wall::solve_wall_building(problem, options),
        ThreeDAlgorithm::ColumnBuilding => column::solve_column_building(problem, options),
        ThreeDAlgorithm::MultiStart => multi_start::solve_multi_start(problem, options),
        ThreeDAlgorithm::Grasp => grasp::solve_grasp(problem, options),
        ThreeDAlgorithm::LocalSearch => local_search::solve_local_search(problem, options),
        ThreeDAlgorithm::FirstFitDecreasingVolume => {
            sorted::solve_first_fit_decreasing_volume(problem, options)
        }
        ThreeDAlgorithm::BestFitDecreasingVolume => {
            sorted::solve_best_fit_decreasing_volume(problem, options)
        }
        ThreeDAlgorithm::Auto => auto::solve_auto(problem, options),
    }
}

fn estimate_required_bin_counts(
    problem: &ThreeDProblem,
    options: &ThreeDOptions,
) -> Result<Vec<usize>> {
    let mut relaxed_problem = problem.clone();
    for bin in &mut relaxed_problem.bins {
        bin.quantity = None;
    }

    let relaxed_options = ThreeDOptions { algorithm: ThreeDAlgorithm::Auto, ..options.clone() };
    let relaxed_solution = solve_3d_core(&relaxed_problem, &relaxed_options)?;
    Ok(count_layouts_by_bin(&relaxed_problem.bins, &relaxed_solution.layouts))
}

fn set_bin_requirements(solution: &mut ThreeDSolution, bins: &[Bin3D], required_counts: &[usize]) {
    let used_counts = count_layouts_by_bin(bins, &solution.layouts);
    solution.bin_requirements = bins
        .iter()
        .enumerate()
        .map(|(index, bin)| {
            let used_quantity = *used_counts.get(index).unwrap_or(&0);
            let required_quantity = *required_counts.get(index).unwrap_or(&used_quantity);
            let additional_quantity_needed = bin
                .quantity
                .map(|available| required_quantity.saturating_sub(available))
                .unwrap_or(0);

            BinRequirement3D {
                bin_name: bin.name.clone(),
                bin_width: bin.width,
                bin_height: bin.height,
                bin_depth: bin.depth,
                cost: bin.cost,
                available_quantity: bin.quantity,
                used_quantity,
                required_quantity,
                additional_quantity_needed,
            }
        })
        .collect();
}

fn count_layouts_by_bin(bins: &[Bin3D], layouts: &[BinLayout3D]) -> Vec<usize> {
    let mut counts = vec![0_usize; bins.len()];
    for layout in layouts {
        if let Some(index) = bins.iter().position(|bin| {
            bin.name == layout.bin_name
                && bin.width == layout.width
                && bin.height == layout.height
                && bin.depth == layout.depth
                && bin.cost == layout.cost
        }) {
            counts[index] = counts[index].saturating_add(1);
        }
    }
    counts
}
