//! WebAssembly bindings for the `bin-packing` crate.
//!
//! Exposes dimension-specific `#[wasm_bindgen]` entry points.
//! The plain-object functions use `serde-wasm-bindgen`, so callers can pass
//! ordinary JavaScript objects without an intermediate JSON string. The default
//! combined build also exposes JSON-string helpers for compatibility with the
//! Node binding; dimension-specific builds omit those helpers to keep browser
//! payloads smaller.
//!
//! `bin-packing` errors are translated into JavaScript exceptions with the original
//! error message preserved.

use serde::Serialize;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[cfg(feature = "one-d")]
use bin_packing::one_d::{OneDOptions, OneDProblem, OneDSolution, solve_1d};
#[cfg(feature = "three-d")]
use bin_packing::three_d::{ThreeDOptions, ThreeDProblem, solve_3d};
#[cfg(feature = "two-d")]
use bin_packing::two_d::{TwoDOptions, TwoDProblem, TwoDSolution, solve_2d};

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
    let solution = from_js_value::<TwoDSolution>(solution, "solution")?;
    let options = options_from_js::<CutPlanOptions2D>(options)?;
    let cut_plan = plan_cuts(&solution, &options).map_err(to_js_error)?;
    to_js_value(&cut_plan)
}

/// Return the crate version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn from_js_value<T: DeserializeOwned>(value: JsValue, field: &str) -> Result<T, JsError> {
    if value.is_undefined() || value.is_null() {
        return Err(JsError::new(&format!("`{field}` is required")));
    }
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| JsError::new(&format!("failed to decode `{field}`: {error}")))
}

fn options_from_js<T: DeserializeOwned + Default>(value: JsValue) -> Result<T, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| JsError::new(&format!("failed to decode `options`: {error}")))
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value)
        .map_err(|error| JsError::new(&format!("failed to encode solution: {error}")))
}

#[cfg(feature = "json")]
fn parse_json<T: DeserializeOwned>(input: &str) -> Result<T, JsError> {
    serde_json::from_str(input).map_err(to_js_error)
}

#[cfg(feature = "json")]
fn options_from_json<T: DeserializeOwned + Default>(input: Option<&str>) -> Result<T, JsError> {
    match input {
        Some(text) => parse_json::<T>(text),
        None => Ok(T::default()),
    }
}

fn to_js_error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}
