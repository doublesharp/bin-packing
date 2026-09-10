//! WebAssembly bindings for the `bin-packing` crate.
//!
//! Exposes dimension-specific `#[wasm_bindgen]` entry points.
//! The plain-object functions bridge through JSON internally so callers can
//! pass ordinary JavaScript objects and receive numeric aggregate metrics. The default
//! combined build also exposes JSON-string helpers for compatibility with the
//! Node binding; dimension-specific builds omit those helpers to keep browser
//! payloads smaller.
//!
//! `bin-packing` errors are translated into JavaScript exceptions with the original
//! error message preserved.

#[cfg(feature = "two-d")]
#[path = "../../two_d_input.rs"]
mod two_d_input;

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
use serde::Serialize;
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[cfg(feature = "one-d")]
use bin_packing::one_d::{OneDOptions, OneDProblem, OneDSolution, solve_1d};
#[cfg(feature = "three-d")]
use bin_packing::three_d::{ThreeDOptions, ThreeDProblem, solve_3d};
#[cfg(feature = "two-d")]
use bin_packing::two_d::{TwoDOptions, TwoDProblem, solve_2d};

/// Solve a 1D cutting-stock problem. Accepts a plain JS object matching the
/// `OneDProblem` shape and an optional options object. Returns the solution
/// as a plain JS object.
#[cfg(feature = "one-d")]
#[wasm_bindgen(js_name = solve1d)]
pub fn solve_1d_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    let problem = from_js_value::<OneDProblem>(problem, "problem")?;
    let options = options_from_js::<OneDOptions>(options)?;
    let solution = solve_1d(problem, options).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Solve a 2D rectangular bin-packing problem. Accepts a plain JS object
/// matching the `TwoDProblem` shape and an optional options object. Returns
/// the solution as a plain JS object.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = solve2d)]
pub fn solve_2d_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    let problem = from_js_value::<TwoDProblem>(problem, "problem")?;
    let options = options_from_js::<TwoDOptions>(options)?;
    let solution = solve_2d(problem, options).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// JSON-string variant of [`solve_1d_js`]. Accepts a JSON-encoded problem and
/// an optional JSON-encoded options string. Returns a JSON-encoded solution
/// string.
#[cfg(all(feature = "one-d", feature = "json"))]
#[wasm_bindgen(js_name = solve1dJson)]
pub fn solve_1d_json(problem_json: &str, options_json: Option<String>) -> Result<String, JsError> {
    let problem = parse_json::<OneDProblem>(problem_json)?;
    let options = options_from_json::<OneDOptions>(options_json.as_deref())?;
    let solution = solve_1d(problem, options).map_err(to_js_error)?;
    serde_json::to_string(&solution).map_err(to_js_error)
}

/// JSON-string variant of [`solve_2d_js`].
#[cfg(all(feature = "two-d", feature = "json"))]
#[wasm_bindgen(js_name = solve2dJson)]
pub fn solve_2d_json(problem_json: &str, options_json: Option<String>) -> Result<String, JsError> {
    let problem = parse_json::<TwoDProblem>(problem_json)?;
    let options = options_from_json::<TwoDOptions>(options_json.as_deref())?;
    let solution = solve_2d(problem, options).map_err(to_js_error)?;
    serde_json::to_string(&solution).map_err(to_js_error)
}

/// Solve a 3D rectangular bin-packing problem. Accepts a plain JS object
/// matching the `ThreeDProblem` shape and an optional options object. Returns
/// the solution as a plain JS object.
#[cfg(feature = "three-d")]
#[wasm_bindgen(js_name = solve3d)]
pub fn solve_3d_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    let problem = from_js_value::<ThreeDProblem>(problem, "problem")?;
    let options = options_from_js::<ThreeDOptions>(options)?;
    let solution = solve_3d(problem, options).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// JSON-string variant of [`solve_3d_js`].
#[cfg(all(feature = "three-d", feature = "json"))]
#[wasm_bindgen(js_name = solve3dJson)]
pub fn solve_3d_json(problem_json: &str, options_json: Option<String>) -> Result<String, JsError> {
    let problem = parse_json::<ThreeDProblem>(problem_json)?;
    let options = options_from_json::<ThreeDOptions>(options_json.as_deref())?;
    let solution = solve_3d(problem, options).map_err(to_js_error)?;
    serde_json::to_string(&solution).map_err(to_js_error)
}

/// Generate a cut plan for a finished 1D solution. Accepts a plain JS object
/// matching the `OneDProblem` shape, a plain JS object matching the
/// `OneDSolution` shape, and an optional options object. Returns the cut plan
/// as a plain JS object.
#[cfg(feature = "one-d")]
#[wasm_bindgen(js_name = plan1dCuts)]
pub fn plan_1d_cuts_js(
    problem: JsValue,
    solution: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    use bin_packing::one_d::cut_plan::{CutPlanOptions1D, plan_cuts};
    let problem = from_js_value::<OneDProblem>(problem, "problem")?;
    let solution = from_js_value::<OneDSolution>(solution, "solution")?;
    let options = options_from_js::<CutPlanOptions1D>(options)?;
    let cut_plan = plan_cuts(&problem, &solution, &options).map_err(to_js_error)?;
    to_js_value(&cut_plan)
}

/// Generate a cut plan for a finished 2D solution. Accepts a plain JS object
/// matching the `TwoDSolution` shape and an optional options object. Returns
/// the cut plan as a plain JS object.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = plan2dCuts)]
pub fn plan_2d_cuts_js(solution: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::two_d::cut_plan::{CutPlanOptions2D, plan_cuts};
    let solution =
        from_js_value::<two_d_input::CutSolutionInput>(solution, "solution")?.into_solution();
    let options = options_from_js::<CutPlanOptions2D>(options)?;
    let cut_plan = plan_cuts(&solution, &options).map_err(to_js_error)?;
    to_js_value(&cut_plan)
}

/// Plan an authoritative workpiece-aware saw job with explicit source indices.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = plan2dSawCuts)]
pub fn plan_2d_saw_cuts_js(job: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::two_d::saw_plan::{SawJob2D, SawPlanOptions2D, plan_saw_cuts};
    let job = from_js_value::<SawJob2D>(job, "job")?;
    let options = options_from_js::<SawPlanOptions2D>(options)?;
    let plan = plan_saw_cuts(&job, &options).map_err(to_js_error)?;
    to_js_value(&plan)
}

/// Return the crate version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// The plain-object entry points bridge through serde_json rather than
// serde-wasm-bindgen so that `u64`/`u128` solution fields behave like the Node
// binding and the published `.d.ts` types: they round-trip as plain JS
// numbers. serde-wasm-bindgen would instead emit `u128` as a `BigInt` (which
// `JSON.stringify` refuses and which the `.d.ts` declares as `number`) and
// would *error* on any `u64` above 2^53 (turning a valid large-coordinate
// solution into a thrown exception). Dimensions are `u32` and always exact;
// only the large aggregate metrics (areas/volumes) are subject to the same
// f64 precision limit they already carry under the JSON contract.
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
fn from_js_value<T: DeserializeOwned>(value: JsValue, field: &str) -> Result<T, JsError> {
    if value.is_undefined() || value.is_null() {
        return Err(JsError::new(&format!("`{field}` is required")));
    }
    let json = js_sys::JSON::stringify(&value).ok().and_then(|text| text.as_string()).ok_or_else(
        || JsError::new(&format!("failed to decode `{field}`: not JSON-serializable")),
    )?;
    serde_json::from_str(&json)
        .map_err(|error| JsError::new(&format!("failed to decode `{field}`: {error}")))
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
fn options_from_js<T: DeserializeOwned + Default>(value: JsValue) -> Result<T, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    let json = js_sys::JSON::stringify(&value)
        .ok()
        .and_then(|text| text.as_string())
        .ok_or_else(|| JsError::new("failed to decode `options`: not JSON-serializable"))?;
    serde_json::from_str(&json)
        .map_err(|error| JsError::new(&format!("failed to decode `options`: {error}")))
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let json = serde_json::to_string(value)
        .map_err(|error| JsError::new(&format!("failed to encode solution: {error}")))?;
    js_sys::JSON::parse(&json)
        .map_err(|_| JsError::new("failed to encode solution: result is not valid JSON"))
}

#[cfg(all(feature = "json", any(feature = "one-d", feature = "two-d", feature = "three-d")))]
fn parse_json<T: DeserializeOwned>(input: &str) -> Result<T, JsError> {
    serde_json::from_str(input).map_err(to_js_error)
}

#[cfg(all(feature = "json", any(feature = "one-d", feature = "two-d", feature = "three-d")))]
fn options_from_json<T: DeserializeOwned + Default>(input: Option<&str>) -> Result<T, JsError> {
    match input {
        Some(text) => parse_json::<T>(text),
        None => Ok(T::default()),
    }
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d"))]
fn to_js_error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}
