#[cfg(feature = "two-d")]
#[path = "../../two_d_input.rs"]
mod two_d_input;

#[cfg(feature = "three-d")]
#[path = "../../number_input.rs"]
mod number_input;

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
use napi::Error;
use napi_derive::napi;
#[cfg(any(feature = "one-d", feature = "two-d", feature = "irregular"))]
use serde::Serialize;
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
use serde::de::DeserializeOwned;

#[napi]
#[cfg(feature = "one-d")]
pub fn solve1d(problem_json: String, options_json: Option<String>) -> napi::Result<String> {
    use bin_packing::one_d::{OneDOptions, OneDProblem, solve_1d};
    let problem = parse_json::<OneDProblem>(&problem_json)?;
    let options =
        options_json.as_deref().map(parse_json::<OneDOptions>).transpose()?.unwrap_or_default();
    let solution = solve_1d(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Minimize material cost after fulfilling as many linear cuts as possible.
#[napi]
#[cfg(feature = "one-d")]
pub fn solve1d_min_cost(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::one_d::{OneDOptions, OneDProblem, solve_1d_min_cost};
    let problem = parse_json::<OneDProblem>(&problem_json)?;
    let options =
        options_json.as_deref().map(parse_json::<OneDOptions>).transpose()?.unwrap_or_default();
    let solution = solve_1d_min_cost(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

#[napi]
#[cfg(feature = "two-d")]
pub fn solve2d(problem_json: String, options_json: Option<String>) -> napi::Result<String> {
    use bin_packing::two_d::{TwoDOptions, TwoDProblem, solve_2d};
    let problem = parse_json::<TwoDProblem>(&problem_json)?;
    let options =
        options_json.as_deref().map(parse_json::<TwoDOptions>).transpose()?.unwrap_or_default();
    let solution = solve_2d(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Pack rectangular sheets with trims, defects, and a selectable objective.
#[napi]
#[cfg(feature = "two-d")]
pub fn solve2d_constrained(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::two_d::constrained::{
        ConstrainedOptions2D, ConstrainedProblem2D, solve_2d_constrained,
    };
    let problem = parse_json::<ConstrainedProblem2D>(&problem_json)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<ConstrainedOptions2D>)
        .transpose()?
        .unwrap_or_default();
    let solution = solve_2d_constrained(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Run the count-bound-aware 2D ensemble.
#[napi]
#[cfg(feature = "two-d")]
pub fn solve2d_adaptive(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::two_d::adaptive::{AdaptiveOptions2D, solve_2d_adaptive};
    let problem = parse_json::<bin_packing::two_d::TwoDProblem>(&problem_json)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<AdaptiveOptions2D>)
        .transpose()?
        .unwrap_or_default();
    let solution = solve_2d_adaptive(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Compute a conservative sheet-count lower bound.
#[napi]
#[cfg(feature = "two-d")]
pub fn lower_bound2d(problem_json: String) -> napi::Result<String> {
    let problem = parse_json::<bin_packing::two_d::TwoDProblem>(&problem_json)?;
    let bound = bin_packing::two_d::lower_bound_2d(&problem).map_err(to_napi_error)?;
    serde_json::to_string(&bound).map_err(to_napi_error)
}

#[napi]
#[cfg(feature = "three-d")]
pub fn solve3d(problem_json: String, options_json: Option<String>) -> napi::Result<String> {
    use bin_packing::three_d::{ThreeDOptions, ThreeDProblem, solve_3d};
    let problem = parse_json::<ThreeDProblem>(&problem_json)?;
    let options =
        options_json.as_deref().map(parse_json::<ThreeDOptions>).transpose()?.unwrap_or_default();
    let solution = solve_3d(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Pack cartons with per-item weights and maximum payloads.
#[napi]
#[cfg(feature = "three-d")]
pub fn solve3d_constrained(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::three_d::constrained::{
        ConstrainedOptions3D, ConstrainedProblem3D, solve_3d_constrained,
    };
    let problem = parse_json::<ConstrainedProblem3D>(&problem_json)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<ConstrainedOptions3D>)
        .transpose()?
        .unwrap_or_default();
    number_input::weights(&problem).map_err(to_napi_error)?;
    let solution = solve_3d_constrained(problem, options).map_err(to_napi_error)?;
    number_input::payloads(&solution.bin_weights).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

#[napi]
#[cfg(feature = "one-d")]
pub fn plan1d_cuts(
    problem_json: String,
    solution_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::one_d::cut_plan::{CutPlanOptions1D, plan_cuts};
    use bin_packing::one_d::{OneDProblem, OneDSolution};
    let problem = parse_json::<OneDProblem>(&problem_json)?;
    let solution = parse_json::<OneDSolution>(&solution_json)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<CutPlanOptions1D>)
        .transpose()?
        .unwrap_or_default();
    let cut_plan = plan_cuts(&problem, &solution, &options).map_err(to_napi_error)?;
    serde_json::to_string(&cut_plan).map_err(to_napi_error)
}

#[napi]
#[cfg(feature = "two-d")]
pub fn plan2d_cuts(solution_json: String, options_json: Option<String>) -> napi::Result<String> {
    use bin_packing::two_d::cut_plan::{CutPlanOptions2D, plan_cuts};
    let solution = parse_json::<two_d_input::CutSolutionInput>(&solution_json)?.into_solution();
    let options = options_json
        .as_deref()
        .map(parse_json::<CutPlanOptions2D>)
        .transpose()?
        .unwrap_or_default();
    let cut_plan = plan_cuts(&solution, &options).map_err(to_napi_error)?;
    serde_json::to_string(&cut_plan).map_err(to_napi_error)
}

/// Plan a workpiece-aware physical saw schedule from JSON with explicit provenance.
#[napi]
#[cfg(feature = "two-d")]
pub fn plan2d_saw_cuts(job_json: String, options_json: Option<String>) -> napi::Result<String> {
    use bin_packing::two_d::saw_plan::{SawJob2D, SawPlanOptions2D, plan_saw_cuts};
    let job = parse_json::<SawJob2D>(&job_json)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<SawPlanOptions2D>)
        .transpose()?
        .unwrap_or_default();
    let plan = plan_saw_cuts(&job, &options).map_err(to_napi_error)?;
    serde_json::to_string(&plan).map_err(to_napi_error)
}

/// Optimize source-indexed profit, material or open linear dimensions.
#[napi]
#[cfg(feature = "one-d")]
pub fn solve1d_optimized(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    run_solver(
        &problem_json,
        options_json.as_deref(),
        bin_packing::one_d::optimization::solve_1d_optimized,
    )
}

/// Optimize source-indexed rectangular demands with physical exclusions.
#[napi]
#[cfg(feature = "two-d")]
pub fn solve2d_optimized(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    run_solver(
        &problem_json,
        options_json.as_deref(),
        bin_packing::two_d::optimization::solve_2d_optimized,
    )
}

/// Construct authoritative physical staged cut trees.
#[napi]
#[cfg(feature = "two-d")]
pub fn solve2d_staged(problem_json: String, options_json: Option<String>) -> napi::Result<String> {
    run_solver(&problem_json, options_json.as_deref(), bin_packing::two_d::staged::solve_2d_staged)
}

/// Optimize profit, material or open dimensions with staged cutting rules.
#[napi]
#[cfg(feature = "two-d")]
pub fn solve2d_staged_optimized(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    run_solver(
        &problem_json,
        options_json.as_deref(),
        bin_packing::two_d::optimization::solve_2d_staged_optimized,
    )
}

/// Optimize payload-aware three-dimensional demands with source provenance.
#[napi]
#[cfg(feature = "three-d")]
pub fn solve3d_optimized(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::three_d::optimization::{OptimizationProblem3D, solve_3d_optimized};
    let problem = parse_json::<OptimizationProblem3D>(&problem_json)?;
    number_input::weights(&problem.packing).map_err(to_napi_error)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<bin_packing::optimization::OptimizationOptions>)
        .transpose()?
        .unwrap_or_default();
    let solution = solve_3d_optimized(problem, options).map_err(to_napi_error)?;
    number_input::indexed_payloads(
        &solution.effective_problem.item_weights,
        &solution.demand_indices,
    )
    .map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Load compatible nested stacks with payload, density, axle and delivery rules.
#[napi]
#[cfg(feature = "three-d")]
pub fn solve3d_loading(problem_json: String, options_json: Option<String>) -> napi::Result<String> {
    use bin_packing::three_d::loading::{LoadingOptions3D, LoadingProblem3D, solve_loading_3d};
    let problem = parse_json::<LoadingProblem3D>(&problem_json)?;
    number_input::loading(&problem).map_err(to_napi_error)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<LoadingOptions3D>)
        .transpose()?
        .unwrap_or_default();
    let solution = solve_loading_3d(problem, options).map_err(to_napi_error)?;
    number_input::loaded(&solution).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Optimize selected loading profit, material or one-container dimensions.
#[napi]
#[cfg(feature = "three-d")]
pub fn solve3d_loading_optimized(
    problem_json: String,
    options_json: Option<String>,
) -> napi::Result<String> {
    use bin_packing::three_d::optimization::{
        OptimizationProblemLoading3D, solve_loading_3d_optimized,
    };
    let problem = parse_json::<OptimizationProblemLoading3D>(&problem_json)?;
    number_input::loading(&problem.packing).map_err(to_napi_error)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<bin_packing::optimization::OptimizationOptions>)
        .transpose()?
        .unwrap_or_default();
    let solution = solve_loading_3d_optimized(problem, options).map_err(to_napi_error)?;
    number_input::loaded(&solution.solution).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

/// Nest rigid polygonal and curved material shapes, with holes and source transforms.
#[napi]
#[cfg(feature = "irregular")]
pub fn solve_irregular(problem_json: String, options_json: Option<String>) -> napi::Result<String> {
    run_solver(&problem_json, options_json.as_deref(), bin_packing::irregular::solve_irregular)
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "irregular"))]
fn run_solver<P: DeserializeOwned, O: DeserializeOwned + Default, S: Serialize>(
    problem_json: &str,
    options_json: Option<&str>,
    solver: impl FnOnce(P, O) -> bin_packing::Result<S>,
) -> napi::Result<String> {
    let problem = parse_json::<P>(problem_json)?;
    let options = options_json.map(parse_json::<O>).transpose()?.unwrap_or_default();
    let solution = solver(problem, options).map_err(to_napi_error)?;
    serde_json::to_string(&solution).map_err(to_napi_error)
}

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn parse_json<T>(input: &str) -> napi::Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(input).map_err(to_napi_error)
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
