//! Run with `cargo run -p bin-packing --example saw_planner`.
use bin_packing::two_d::saw_plan::{SawJob2D, SawLayout2D, SawPlanOptions2D, plan_saw_cuts};
use bin_packing::two_d::{
    RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, solve_2d,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let problem = TwoDProblem {
        sheets: vec![Sheet2D {
            name: "plywood".into(),
            width: 1209,
            height: 600,
            cost: 1.0,
            quantity: Some(1),
            kerf: 3,
            edge_kerf_relief: false,
        }],
        demands: vec![RectDemand2D {
            name: "panel".into(),
            width: 300,
            height: 600,
            quantity: 4,
            can_rotate: false,
        }],
    };
    let solution = solve_2d(
        problem.clone(),
        TwoDOptions { algorithm: TwoDAlgorithm::Guillotine, ..Default::default() },
    )?;
    let job = SawJob2D {
        problem,
        layouts: solution.layouts.iter().map(SawLayout2D::from).collect(),
        stock_indices: vec![0; solution.layouts.len()],
        demand_indices: solution.layouts.iter().map(|l| vec![0; l.placements.len()]).collect(),
    };
    let plan = plan_saw_cuts(&job, &SawPlanOptions2D::default())?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}
