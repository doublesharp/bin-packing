#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
use napi::Error;
use napi_derive::napi;
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
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
    use bin_packing::two_d::TwoDSolution;
    use bin_packing::two_d::cut_plan::{CutPlanOptions2D, plan_cuts};
    let solution = parse_json::<TwoDSolution>(&solution_json)?;
    let options = options_json
        .as_deref()
        .map(parse_json::<CutPlanOptions2D>)
        .transpose()?
        .unwrap_or_default();
    let cut_plan = plan_cuts(&solution, &options).map_err(to_napi_error)?;
    serde_json::to_string(&cut_plan).map_err(to_napi_error)
}

#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
fn parse_json<T>(input: &str) -> napi::Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(input).map_err(to_napi_error)
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
fn to_napi_error(error: impl std::fmt::Display) -> Error {
    Error::from_reason(error.to_string())
}
