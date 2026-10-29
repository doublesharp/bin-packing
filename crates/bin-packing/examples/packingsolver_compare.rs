//! JSON-in/JSON-out release harness for the independent PackingSolver comparison.

use std::error::Error;
use std::io::{self, Read, Write};
use std::time::Instant;

use bin_packing::three_d::constrained::{
    ConstrainedOptions3D, ConstrainedProblem3D, solve_3d_constrained,
};
use bin_packing::three_d::{ThreeDOptions, ThreeDProblem, solve_3d};
use bin_packing::two_d::adaptive::{AdaptiveOptions2D, solve_2d_adaptive};
use bin_packing::two_d::constrained::{
    ConstrainedOptions2D, ConstrainedProblem2D, solve_2d_constrained,
};
use bin_packing::two_d::{TwoDOptions, TwoDProblem, solve_2d};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type HarnessResult<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize)]
struct Request {
    dimension: String,
    problem: Value,
    options: Value,
    warmups: usize,
    repeats: usize,
    max_states: usize,
}

fn measure<T: Serialize>(
    variant: &str,
    warmups: usize,
    repeats: usize,
    mut solve: impl FnMut() -> bin_packing::Result<T>,
) -> HarnessResult<Value> {
    for _ in 0..warmups {
        solve()?;
    }
    let mut samples = Vec::with_capacity(repeats);
    let mut final_result = None;
    for _ in 0..repeats {
        let start = Instant::now();
        let result = solve()?;
        let elapsed = start.elapsed().as_nanos();
        // Conversion and deterministic-output comparison are outside solve time.
        let value = serde_json::to_value(result)?;
        if final_result.as_ref().is_some_and(|previous| *previous != value) {
            return Err(
                format!("{variant} returned different results for identical seeded input").into()
            );
        }
        final_result = Some(value);
        samples.push(elapsed);
    }
    let mut ordered = samples.clone();
    ordered.sort_unstable();
    Ok(json!({
        "variant": variant,
        "samples_ns": samples,
        "median_ns": ordered[ordered.len() / 2],
        "result": final_result.ok_or("no measured solver result")?,
    }))
}

fn main() -> HarnessResult<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    if !(1..=10).contains(&request.warmups) || !(1..=51).contains(&request.repeats) {
        return Err("warmups must be 1..=10 and repeats 1..=51".into());
    }
    let variants = match request.dimension.as_str() {
        "one_d" => {
            let problem =
                serde_json::from_value::<bin_packing::one_d::OneDProblem>(request.problem)?;
            let options =
                serde_json::from_value::<bin_packing::one_d::OneDOptions>(request.options)?;
            vec![measure("min_cost", request.warmups, request.repeats, || {
                bin_packing::one_d::solve_1d_min_cost(problem.clone(), options.clone())
            })?]
        }
        "one_d_optimized" => {
            use bin_packing::one_d::optimization::{
                OptimizationOptions, OptimizationProblem1D, solve_1d_optimized,
            };
            let problem = serde_json::from_value::<OptimizationProblem1D>(request.problem)?;
            let options = serde_json::from_value::<OptimizationOptions>(request.options)?;
            vec![measure("optimized", request.warmups, request.repeats, || {
                solve_1d_optimized(problem.clone(), options.clone())
            })?]
        }
        "two_d_optimized" => {
            use bin_packing::two_d::optimization::{
                OptimizationOptions, OptimizationProblem2D, solve_2d_optimized,
            };
            let problem = serde_json::from_value::<OptimizationProblem2D>(request.problem)?;
            let options = serde_json::from_value::<OptimizationOptions>(request.options)?;
            vec![measure("optimized", request.warmups, request.repeats, || {
                solve_2d_optimized(problem.clone(), options.clone())
            })?]
        }
        "staged" => {
            use bin_packing::two_d::staged::{StagedOptions2D, StagedProblem2D, solve_2d_staged};
            let problem = serde_json::from_value::<StagedProblem2D>(request.problem)?;
            let options = serde_json::from_value::<StagedOptions2D>(request.options)?;
            vec![measure("staged", request.warmups, request.repeats, || {
                solve_2d_staged(problem.clone(), options.clone())
            })?]
        }
        "staged_optimized" => {
            use bin_packing::two_d::optimization::{
                OptimizationOptions, OptimizationProblemStaged2D, solve_2d_staged_optimized,
            };
            let problem = serde_json::from_value::<OptimizationProblemStaged2D>(request.problem)?;
            let options = serde_json::from_value::<OptimizationOptions>(request.options)?;
            vec![measure("optimized", request.warmups, request.repeats, || {
                solve_2d_staged_optimized(problem.clone(), options.clone())
            })?]
        }
        "three_d_optimized" => {
            use bin_packing::three_d::optimization::{
                OptimizationOptions, OptimizationProblem3D, solve_3d_optimized,
            };
            let problem = serde_json::from_value::<OptimizationProblem3D>(request.problem)?;
            let options = serde_json::from_value::<OptimizationOptions>(request.options)?;
            vec![measure("optimized", request.warmups, request.repeats, || {
                solve_3d_optimized(problem.clone(), options.clone())
            })?]
        }
        "loading" => {
            use bin_packing::three_d::loading::{
                LoadingOptions3D, LoadingProblem3D, solve_loading_3d,
            };
            let problem = serde_json::from_value::<LoadingProblem3D>(request.problem)?;
            let options = serde_json::from_value::<LoadingOptions3D>(request.options)?;
            vec![measure("loading", request.warmups, request.repeats, || {
                solve_loading_3d(problem.clone(), options.clone())
            })?]
        }
        "loading_optimized" => {
            use bin_packing::three_d::optimization::{
                OptimizationOptions, OptimizationProblemLoading3D, solve_loading_3d_optimized,
            };
            let problem = serde_json::from_value::<OptimizationProblemLoading3D>(request.problem)?;
            let options = serde_json::from_value::<OptimizationOptions>(request.options)?;
            vec![measure("optimized", request.warmups, request.repeats, || {
                solve_loading_3d_optimized(problem.clone(), options.clone())
            })?]
        }
        "irregular" => {
            use bin_packing::irregular::{IrregularOptions, IrregularProblem, solve_irregular};
            let problem = serde_json::from_value::<IrregularProblem>(request.problem)?;
            let options = serde_json::from_value::<IrregularOptions>(request.options)?;
            vec![measure("irregular", request.warmups, request.repeats, || {
                solve_irregular(problem.clone(), options.clone())
            })?]
        }
        "two_d" => {
            let problem: TwoDProblem = serde_json::from_value(request.problem)?;
            let options: TwoDOptions = serde_json::from_value(request.options)?;
            let adaptive = AdaptiveOptions2D { solver: options.clone(), ..Default::default() };
            let constrained =
                ConstrainedProblem2D { problem: problem.clone(), constraints: vec![] };
            let constrained_options = ConstrainedOptions2D {
                beam_width: options.beam_width,
                max_states: request.max_states,
                ..Default::default()
            };
            vec![
                measure("ordinary", request.warmups, request.repeats, || {
                    solve_2d(problem.clone(), options.clone())
                })?,
                measure("adaptive", request.warmups, request.repeats, || {
                    solve_2d_adaptive(problem.clone(), adaptive.clone())
                })?,
                measure("constrained", request.warmups, request.repeats, || {
                    solve_2d_constrained(constrained.clone(), constrained_options.clone())
                })?,
            ]
        }
        "three_d" => {
            let problem: ThreeDProblem = serde_json::from_value(request.problem)?;
            let options: ThreeDOptions = serde_json::from_value(request.options)?;
            let constrained = ConstrainedProblem3D {
                problem: problem.clone(),
                item_weights: vec![],
                bin_max_weights: vec![],
            };
            let constrained_options = ConstrainedOptions3D {
                beam_width: options.beam_width,
                max_states: request.max_states,
                ..Default::default()
            };
            vec![
                measure("ordinary", request.warmups, request.repeats, || {
                    solve_3d(problem.clone(), options.clone())
                })?,
                measure("constrained", request.warmups, request.repeats, || {
                    solve_3d_constrained(constrained.clone(), constrained_options.clone())
                })?,
            ]
        }
        dimension => return Err(format!("unknown dimension {dimension:?}").into()),
    };
    let output = json!({
        "release_build": !cfg!(debug_assertions),
        "crate_version": env!("CARGO_PKG_VERSION"),
        "compiled_features": {
            "one_d": cfg!(feature = "one-d"),
            "two_d": cfg!(feature = "two-d"),
            "three_d": cfg!(feature = "three-d"),
            "irregular": cfg!(feature = "irregular"),
            "parallel": cfg!(feature = "parallel"),
        },
        "dimension": request.dimension,
        "warmups": request.warmups,
        "repeats": request.repeats,
        "max_states": request.max_states,
        "method": "one or more untimed warmups; serial release solves; cloning included; parse, serialization, output comparison and compilation excluded",
        "variants": variants,
    });
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &output)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
