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

#[cfg(feature = "three-d")]
#[path = "../../number_input.rs"]
mod number_input;

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
use serde::Serialize;
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
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

/// Minimize material cost with the bounded multi-stock 1D pattern search.
#[cfg(feature = "one-d")]
#[wasm_bindgen(js_name = solve1dMinCost)]
pub fn solve_1d_min_cost_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    let problem = from_js_value::<OneDProblem>(problem, "problem")?;
    let options = options_from_js::<OneDOptions>(options)?;
    let solution = bin_packing::one_d::solve_1d_min_cost(problem, options).map_err(to_js_error)?;
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

/// Pack rectangular sheets while enforcing trims and rectangular defects.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = solve2dConstrained)]
pub fn solve_2d_constrained_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::two_d::constrained::{
        ConstrainedOptions2D, ConstrainedProblem2D, solve_2d_constrained,
    };
    let problem = from_js_value::<ConstrainedProblem2D>(problem, "problem")?;
    let options = options_from_js::<ConstrainedOptions2D>(options)?;
    let solution = solve_2d_constrained(problem, options).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Run the count-bound-aware 2D ensemble and return its count certificate.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = solve2dAdaptive)]
pub fn solve_2d_adaptive_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::two_d::adaptive::{AdaptiveOptions2D, solve_2d_adaptive};
    let problem = from_js_value::<TwoDProblem>(problem, "problem")?;
    let options = options_from_js::<AdaptiveOptions2D>(options)?;
    let solution = solve_2d_adaptive(problem, options).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Compute a conservative lower bound on sheets needed for every demand.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = lowerBound2d)]
pub fn lower_bound_2d_js(problem: JsValue) -> Result<usize, JsError> {
    let problem = from_js_value::<TwoDProblem>(problem, "problem")?;
    bin_packing::two_d::lower_bound_2d(&problem).map_err(to_js_error)
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

/// Pack cartons with payload limits. Weights must be safe JavaScript integers.
#[cfg(feature = "three-d")]
#[wasm_bindgen(js_name = solve3dConstrained)]
pub fn solve_3d_constrained_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::three_d::constrained::{
        ConstrainedOptions3D, ConstrainedProblem3D, solve_3d_constrained,
    };
    let problem = from_js_value::<ConstrainedProblem3D>(problem, "problem")?;
    number_input::weights(&problem).map_err(to_js_error)?;
    let options = options_from_js::<ConstrainedOptions3D>(options)?;
    let solution = solve_3d_constrained(problem, options).map_err(to_js_error)?;
    number_input::payloads(&solution.bin_weights).map_err(to_js_error)?;
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

/// Optimize indexed linear profit, material or open dimensions.
#[cfg(feature = "one-d")]
#[wasm_bindgen(js_name = solve1dOptimized)]
pub fn solve_1d_optimized_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    run_solver(problem, options, bin_packing::one_d::optimization::solve_1d_optimized)
}

/// Optimize rectangular profit, material or open dimensions with exclusions.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = solve2dOptimized)]
pub fn solve_2d_optimized_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    run_solver(problem, options, bin_packing::two_d::optimization::solve_2d_optimized)
}

/// Construct authoritative staged physical guillotine trees.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = solve2dStaged)]
pub fn solve_2d_staged_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    run_solver(problem, options, bin_packing::two_d::staged::solve_2d_staged)
}

/// Optimize profit, material or open dimensions while preserving staged rules.
#[cfg(feature = "two-d")]
#[wasm_bindgen(js_name = solve2dStagedOptimized)]
pub fn solve_2d_staged_optimized_js(
    problem: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    run_solver(problem, options, bin_packing::two_d::optimization::solve_2d_staged_optimized)
}

/// Optimize spatial demands with exact safe-integer payload accounting.
#[cfg(feature = "three-d")]
#[wasm_bindgen(js_name = solve3dOptimized)]
pub fn solve_3d_optimized_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::three_d::optimization::{OptimizationProblem3D, solve_3d_optimized};
    let problem = from_js_value::<OptimizationProblem3D>(problem, "problem")?;
    number_input::weights(&problem.packing).map_err(to_js_error)?;
    let options = options_from_js::<bin_packing::optimization::OptimizationOptions>(options)?;
    let solution = solve_3d_optimized(problem, options).map_err(to_js_error)?;
    number_input::indexed_payloads(
        &solution.effective_problem.item_weights,
        &solution.demand_indices,
    )
    .map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Load compatible nested stacks under payload, density, axle and delivery rules.
#[cfg(feature = "three-d")]
#[wasm_bindgen(js_name = solve3dLoading)]
pub fn solve_3d_loading_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    use bin_packing::three_d::loading::{LoadingOptions3D, LoadingProblem3D, solve_loading_3d};
    let problem = from_js_value::<LoadingProblem3D>(problem, "problem")?;
    number_input::loading(&problem).map_err(to_js_error)?;
    let options = options_from_js::<LoadingOptions3D>(options)?;
    let solution = solve_loading_3d(problem, options).map_err(to_js_error)?;
    number_input::loaded(&solution).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Optimize indexed loading profit, material or one-container dimensions.
#[cfg(feature = "three-d")]
#[wasm_bindgen(js_name = solve3dLoadingOptimized)]
pub fn solve_3d_loading_optimized_js(
    problem: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    use bin_packing::three_d::optimization::{
        OptimizationProblemLoading3D, solve_loading_3d_optimized,
    };
    let problem = from_js_value::<OptimizationProblemLoading3D>(problem, "problem")?;
    number_input::loading(&problem.packing).map_err(to_js_error)?;
    let options = options_from_js::<bin_packing::optimization::OptimizationOptions>(options)?;
    let solution = solve_loading_3d_optimized(problem, options).map_err(to_js_error)?;
    number_input::loaded(&solution.solution).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Nest rigid polygonal and curved shapes with holes and original transforms.
#[cfg(feature = "irregular")]
#[wasm_bindgen(js_name = solveIrregular)]
pub fn solve_irregular_js(problem: JsValue, options: JsValue) -> Result<JsValue, JsError> {
    run_solver(problem, options, bin_packing::irregular::solve_irregular)
}

/// JSON-string variant of the native irregular nesting entry point.
#[cfg(all(feature = "irregular", feature = "json"))]
#[wasm_bindgen(js_name = solveIrregularJson)]
pub fn solve_irregular_json(
    problem_json: &str,
    options_json: Option<String>,
) -> Result<String, JsError> {
    let problem = parse_json::<bin_packing::irregular::IrregularProblem>(problem_json)?;
    let options =
        options_from_json::<bin_packing::irregular::IrregularOptions>(options_json.as_deref())?;
    let solution =
        bin_packing::irregular::solve_irregular(problem, options).map_err(to_js_error)?;
    serde_json::to_string(&solution).map_err(to_js_error)
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "irregular"))]
fn run_solver<P: DeserializeOwned, O: DeserializeOwned + Default, S: Serialize>(
    problem: JsValue,
    options: JsValue,
    solver: impl FnOnce(P, O) -> bin_packing::Result<S>,
) -> Result<JsValue, JsError> {
    let problem = from_js_value::<P>(problem, "problem")?;
    let options = options_from_js::<O>(options)?;
    let solution = solver(problem, options).map_err(to_js_error)?;
    to_js_value(&solution)
}

/// Return the crate version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// Inspect before JSON.stringify can turn non-finite limits into null/unlimited.
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn check_js_numbers(value: &JsValue, field: &str, depth: usize) -> Result<(), JsError> {
    if depth > 128 {
        return Err(JsError::new("input exceeds the supported JSON nesting depth"));
    }
    let scalar = matches!(
        field,
        "maximum_weight_above"
            | "numerator"
            | "denominator"
            | "front_maximum"
            | "rear_maximum"
            | "front_tare"
            | "rear_tare"
            | "seed"
    );
    if scalar {
        let nullable = matches!(field, "maximum_weight_above" | "seed");
        check_safe_number(value, nullable)?;
    }
    if matches!(field, "item_weights" | "bin_max_weights") && !value.is_undefined() {
        if !js_sys::Array::is_array(value) {
            return Err(JsError::new(
                "weights must be arrays of nonnegative JavaScript safe integers",
            ));
        }
        for item in js_sys::Array::from(value).iter() {
            check_safe_number(&item, field == "bin_max_weights")?;
        }
    }
    if value.as_f64().is_some_and(|number| !number.is_finite()) {
        return Err(JsError::new("all numeric input fields must be finite"));
    }
    if js_sys::Array::is_array(value) {
        for item in js_sys::Array::from(value).iter() {
            check_js_numbers(&item, "", depth + 1)?;
        }
    } else if value.is_object() && !value.is_null() {
        for entry in js_sys::Object::entries(&js_sys::Object::from(value.clone())).iter() {
            let pair = js_sys::Array::from(&entry);
            let key =
                pair.get(0).as_string().ok_or_else(|| JsError::new("invalid JSON object key"))?;
            check_js_numbers(&pair.get(1), &key, depth + 1)?;
        }
    }
    Ok(())
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn check_safe_number(value: &JsValue, nullable: bool) -> Result<(), JsError> {
    if nullable && value.is_null() {
        return Ok(());
    }
    if value.as_f64().is_none_or(|number| {
        !number.is_finite()
            || number < 0.0
            || number.fract() != 0.0
            || number > 9_007_199_254_740_991.0
    }) {
        return Err(JsError::new(
            "weights and loading limits must be nonnegative JavaScript safe integers",
        ));
    }
    Ok(())
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn stringify_checked(value: &JsValue, field: &str) -> Result<String, JsError> {
    check_js_numbers(value, "", 0)?;
    // A getter or toJSON method may introduce a different value after the
    // initial walk. The replacer observes it before JSON can coerce it to null.
    let replacer = Closure::<dyn FnMut(JsValue, JsValue) -> Result<JsValue, JsError>>::new(
        |key: JsValue, value: JsValue| {
            check_js_numbers(&value, key.as_string().as_deref().unwrap_or(""), 0)?;
            Ok(value)
        },
    );
    let message = format!(
        "failed to decode `{field}`: input must be JSON-serializable with finite numbers and safe integer loading limits"
    );
    js_sys::JSON::stringify_with_replacer(value, replacer.as_ref())
        .map_err(|_| JsError::new(&message))?
        .as_string()
        .ok_or_else(|| JsError::new(&message))
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
#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn from_js_value<T: DeserializeOwned>(value: JsValue, field: &str) -> Result<T, JsError> {
    if value.is_undefined() || value.is_null() {
        return Err(JsError::new(&format!("`{field}` is required")));
    }
    let json = stringify_checked(&value, field)?;
    serde_json::from_str(&json)
        .map_err(|error| JsError::new(&format!("failed to decode `{field}`: {error}")))
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn options_from_js<T: DeserializeOwned + Default>(value: JsValue) -> Result<T, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(T::default());
    }
    let json = stringify_checked(&value, "options")?;
    serde_json::from_str(&json)
        .map_err(|error| JsError::new(&format!("failed to decode `options`: {error}")))
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let json = serde_json::to_string(value)
        .map_err(|error| JsError::new(&format!("failed to encode solution: {error}")))?;
    js_sys::JSON::parse(&json)
        .map_err(|_| JsError::new("failed to encode solution: result is not valid JSON"))
}

#[cfg(all(
    feature = "json",
    any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular")
))]
fn parse_json<T: DeserializeOwned>(input: &str) -> Result<T, JsError> {
    serde_json::from_str(input).map_err(to_js_error)
}

#[cfg(all(
    feature = "json",
    any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular")
))]
fn options_from_json<T: DeserializeOwned + Default>(input: Option<&str>) -> Result<T, JsError> {
    match input {
        Some(text) => parse_json::<T>(text),
        None => Ok(T::default()),
    }
}

#[cfg(any(feature = "one-d", feature = "two-d", feature = "three-d", feature = "irregular"))]
fn to_js_error(error: impl std::fmt::Display) -> JsError {
    JsError::new(&error.to_string())
}
