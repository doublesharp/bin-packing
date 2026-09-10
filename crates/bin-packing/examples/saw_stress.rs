//! Resource probe for a 512-part job at the maximum optimization and beam limits.
//! Build in release mode, then run with the platform's process resource profiler.
use bin_packing::two_d::saw_plan::{
    SawJob2D, SawLayout2D, SawPlanOptions2D, SawSearchLimits2D, plan_saw_cuts,
};
use bin_packing::two_d::{Placement2D, RectDemand2D, Sheet2D, TwoDProblem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = 512;
    let job = SawJob2D {
        problem: TwoDProblem {
            sheets: vec![Sheet2D {
                name: "stock".into(),
                width: count,
                height: 10,
                cost: 1.0,
                quantity: None,
                kerf: 0,
                edge_kerf_relief: false,
            }],
            demands: vec![RectDemand2D {
                name: "strip".into(),
                width: 1,
                height: 10,
                quantity: count as usize,
                can_rotate: false,
            }],
        },
        layouts: vec![SawLayout2D {
            width: count,
            height: 10,
            placements: (0..count)
                .map(|x| Placement2D {
                    name: "strip".into(),
                    x,
                    y: 0,
                    width: 1,
                    height: 10,
                    rotated: false,
                })
                .collect(),
        }],
        stock_indices: vec![0],
        demand_indices: vec![vec![0; count as usize]],
    };
    let plan = plan_saw_cuts(
        &job,
        &SawPlanOptions2D {
            search: SawSearchLimits2D {
                max_states: 100_000,
                beam_width: 256,
                ..Default::default()
            },
            ..Default::default()
        },
    )?;
    println!("{} cuts, {} optimization states", plan.counts.cuts, plan.search.explored_states);
    Ok(())
}
