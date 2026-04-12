#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use bin_packing::BinPackingError;
use bin_packing::three_d::{
    Bin3D, BinLayout3D, BoxDemand3D, Placement3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions,
    ThreeDProblem, ThreeDSolution, solve_3d,
};
use libfuzzer_sys::fuzz_target;

const MAX_BIN_TYPES: usize = 3;
const MAX_DEMAND_TYPES: usize = 7;
const MAX_DIMENSION: u32 = 72;
const MAX_QUANTITY: usize = 4;
const MAX_AVAILABLE: usize = 5;

#[derive(Debug, Arbitrary)]
struct BinSeed {
    width: u16,
    height: u16,
    depth: u16,
    cost_units: u8,
    quantity: Option<u8>,
}

#[derive(Debug, Arbitrary)]
struct DemandSeed {
    width: u16,
    height: u16,
    depth: u16,
    quantity: u8,
    rotation_mask: u8,
    duplicate_name: bool,
}

#[derive(Debug, Arbitrary)]
struct ThreeDModuleSeed {
    bins: Vec<BinSeed>,
    demands: Vec<DemandSeed>,
    algorithm: u8,
    seed: u64,
    multistart_runs: u8,
    improvement_rounds: u8,
    beam_width: u8,
    guillotine_required: bool,
    branch_and_bound_node_limit: u16,
}

fn map_algorithm(value: u8) -> ThreeDAlgorithm {
    match value % 29 {
        0 => ThreeDAlgorithm::Auto,
        1 => ThreeDAlgorithm::ExtremePoints,
        2 => ThreeDAlgorithm::ExtremePointsResidualSpace,
        3 => ThreeDAlgorithm::ExtremePointsFreeVolume,
        4 => ThreeDAlgorithm::ExtremePointsBottomLeftBack,
        5 => ThreeDAlgorithm::ExtremePointsContactPoint,
        6 => ThreeDAlgorithm::ExtremePointsEuclidean,
        7 => ThreeDAlgorithm::Guillotine3D,
        8 => ThreeDAlgorithm::Guillotine3DBestShortSideFit,
        9 => ThreeDAlgorithm::Guillotine3DBestLongSideFit,
        10 => ThreeDAlgorithm::Guillotine3DShorterLeftoverAxis,
        11 => ThreeDAlgorithm::Guillotine3DLongerLeftoverAxis,
        12 => ThreeDAlgorithm::Guillotine3DMinVolumeSplit,
        13 => ThreeDAlgorithm::Guillotine3DMaxVolumeSplit,
        14 => ThreeDAlgorithm::LayerBuilding,
        15 => ThreeDAlgorithm::LayerBuildingMaxRects,
        16 => ThreeDAlgorithm::LayerBuildingSkyline,
        17 => ThreeDAlgorithm::LayerBuildingGuillotine,
        18 => ThreeDAlgorithm::LayerBuildingShelf,
        19 => ThreeDAlgorithm::WallBuilding,
        20 => ThreeDAlgorithm::ColumnBuilding,
        21 => ThreeDAlgorithm::DeepestBottomLeft,
        22 => ThreeDAlgorithm::DeepestBottomLeftFill,
        23 => ThreeDAlgorithm::FirstFitDecreasingVolume,
        24 => ThreeDAlgorithm::BestFitDecreasingVolume,
        25 => ThreeDAlgorithm::MultiStart,
        26 => ThreeDAlgorithm::Grasp,
        27 => ThreeDAlgorithm::LocalSearch,
        _ => ThreeDAlgorithm::BranchAndBound,
    }
}

fn map_rotation_mask(value: u8) -> RotationMask3D {
    match value % 9 {
        0 => RotationMask3D::XYZ,
        1 => RotationMask3D::XZY,
        2 => RotationMask3D::YXZ,
        3 => RotationMask3D::YZX,
        4 => RotationMask3D::ZXY,
        5 => RotationMask3D::ZYX,
        6 => RotationMask3D::UPRIGHT,
        _ => RotationMask3D::ALL,
    }
}

fn build_problem(input: &ThreeDModuleSeed, algorithm: ThreeDAlgorithm) -> ThreeDProblem {
    let bin_limit = if algorithm == ThreeDAlgorithm::BranchAndBound { 1 } else { MAX_BIN_TYPES };
    let mut bins = input
        .bins
        .iter()
        .take(bin_limit)
        .enumerate()
        .map(|(index, seed)| build_bin(index, seed, algorithm))
        .collect::<Vec<_>>();

    if bins.is_empty() {
        bins.push(Bin3D {
            name: "bin_0".to_string(),
            width: 32,
            height: 32,
            depth: 32,
            cost: 1.0,
            quantity: None,
        });
    }

    let demands = input
        .demands
        .iter()
        .take(MAX_DEMAND_TYPES)
        .enumerate()
        .map(|(index, seed)| build_demand(index, seed, &bins, algorithm))
        .collect::<Vec<_>>();

    let demands = if demands.is_empty() {
        vec![BoxDemand3D {
            name: "item_0".to_string(),
            width: 8,
            height: 8,
            depth: 8,
            quantity: 1,
            allowed_rotations: rotation_mask_for_algorithm(RotationMask3D::ALL, algorithm),
        }]
    } else {
        demands
    };

    ThreeDProblem { bins, demands }
}

fn build_bin(index: usize, seed: &BinSeed, algorithm: ThreeDAlgorithm) -> Bin3D {
    let quantity = if algorithm == ThreeDAlgorithm::BranchAndBound {
        None
    } else {
        seed.quantity.map(|raw| usize::from(raw % ((MAX_AVAILABLE + 1) as u8)).max(1))
    };

    Bin3D {
        name: format!("bin_{index}"),
        width: u32::from(seed.width).clamp(8, MAX_DIMENSION),
        height: u32::from(seed.height).clamp(8, MAX_DIMENSION),
        depth: u32::from(seed.depth).clamp(8, MAX_DIMENSION),
        cost: 1.0 + f64::from(seed.cost_units % 8),
        quantity,
    }
}

fn build_demand(
    index: usize,
    seed: &DemandSeed,
    bins: &[Bin3D],
    algorithm: ThreeDAlgorithm,
) -> BoxDemand3D {
    let allowed_rotations =
        rotation_mask_for_algorithm(map_rotation_mask(seed.rotation_mask), algorithm);
    let (width, height, depth) = clamp_demand_extents(seed, bins, allowed_rotations);
    let name =
        if seed.duplicate_name { format!("dup_{}", index % 2) } else { format!("item_{index}") };

    BoxDemand3D {
        name,
        width,
        height,
        depth,
        quantity: usize::from(seed.quantity % (MAX_QUANTITY as u8)).max(1),
        allowed_rotations,
    }
}

fn rotation_mask_for_algorithm(mask: RotationMask3D, algorithm: ThreeDAlgorithm) -> RotationMask3D {
    if algorithm == ThreeDAlgorithm::BranchAndBound { RotationMask3D::XYZ } else { mask }
}

fn clamp_demand_extents(
    seed: &DemandSeed,
    bins: &[Bin3D],
    allowed_rotations: RotationMask3D,
) -> (u32, u32, u32) {
    let max_fit =
        bins.iter().flat_map(|bin| [bin.width, bin.height, bin.depth]).min().unwrap_or(8).max(1);
    let mut extents = (
        u32::from(seed.width.max(1)).min(max_fit),
        u32::from(seed.height.max(1)).min(max_fit),
        u32::from(seed.depth.max(1)).min(max_fit),
    );

    if fits_some_bin(extents, bins, allowed_rotations) {
        return extents;
    }

    let fallback = max_fit.clamp(1, 8);
    extents = (fallback, fallback, fallback);
    extents
}

fn fits_some_bin(
    extents: (u32, u32, u32),
    bins: &[Bin3D],
    allowed_rotations: RotationMask3D,
) -> bool {
    allowed_rotations.iter().any(|rotation| {
        let (width, height, depth) = rotation.apply(extents.0, extents.1, extents.2);
        bins.iter().any(|bin| width <= bin.width && height <= bin.height && depth <= bin.depth)
    })
}

fn assert_three_d_solution(problem: &ThreeDProblem, solution: &ThreeDSolution) {
    let mut remaining_by_demand =
        problem.demands.iter().map(|demand| demand.quantity).collect::<Vec<_>>();
    let mut layout_counts = vec![0_usize; problem.bins.len()];
    let mut total_waste_volume = 0_u64;
    let mut total_cost = 0.0_f64;

    for layout in &solution.layouts {
        let bin_index = matching_bin_index(problem, layout);
        let bin = &problem.bins[bin_index];
        layout_counts[bin_index] = layout_counts[bin_index].saturating_add(1);
        total_cost += layout.cost;

        assert_non_overlapping(&layout.placements);

        let mut used_volume = 0_u64;
        for placement in &layout.placements {
            assert!(
                placement.x.saturating_add(placement.width) <= bin.width,
                "placement exceeds bin width: {placement:?} in {layout:?}",
            );
            assert!(
                placement.y.saturating_add(placement.height) <= bin.height,
                "placement exceeds bin height: {placement:?} in {layout:?}",
            );
            assert!(
                placement.z.saturating_add(placement.depth) <= bin.depth,
                "placement exceeds bin depth: {placement:?} in {layout:?}",
            );
            assert!(
                placement.width > 0 && placement.height > 0 && placement.depth > 0,
                "zero-dimension placement: {placement:?}",
            );

            let demand_index = matching_demand_index(problem, &mut remaining_by_demand, placement);
            remaining_by_demand[demand_index] = remaining_by_demand[demand_index].saturating_sub(1);
            used_volume = used_volume.saturating_add(volume(
                placement.width,
                placement.height,
                placement.depth,
            ));
        }

        let bin_volume = volume(bin.width, bin.height, bin.depth);
        assert_eq!(layout.used_volume, used_volume, "layout used_volume mismatch");
        assert_eq!(
            layout.waste_volume,
            bin_volume.saturating_sub(used_volume),
            "layout waste_volume mismatch",
        );
        total_waste_volume = total_waste_volume.saturating_add(layout.waste_volume);
    }

    for item in &solution.unplaced {
        let demand_index = matching_unplaced_demand_index(problem, &remaining_by_demand, item);
        assert!(
            remaining_by_demand[demand_index] >= item.quantity,
            "unplaced demand exceeds remaining quantity: {item:?}",
        );
        remaining_by_demand[demand_index] =
            remaining_by_demand[demand_index].saturating_sub(item.quantity);
    }

    assert!(
        remaining_by_demand.iter().all(|count| *count == 0),
        "not every demand was accounted for: {remaining_by_demand:?}",
    );
    assert_eq!(solution.layouts.len(), solution.bin_count);
    assert_eq!(solution.total_waste_volume, total_waste_volume);
    assert_close_enough(solution.total_cost, total_cost, solution.layouts.len(), "3D total_cost");

    for (index, bin) in problem.bins.iter().enumerate() {
        if let Some(cap) = bin.quantity {
            assert!(
                layout_counts[index] <= cap,
                "bin {} exceeds cap: used={} cap={}",
                bin.name,
                layout_counts[index],
                cap,
            );
        }
    }

    assert_bin_requirements(problem, solution, &layout_counts);
}

fn matching_bin_index(problem: &ThreeDProblem, layout: &BinLayout3D) -> usize {
    for (index, bin) in problem.bins.iter().enumerate() {
        if bin.name == layout.bin_name
            && bin.width == layout.width
            && bin.height == layout.height
            && bin.depth == layout.depth
            && bin.cost == layout.cost
        {
            return index;
        }
    }
    panic!("layout references an undeclared bin: {layout:?}");
}

fn matching_demand_index(
    problem: &ThreeDProblem,
    remaining_by_demand: &mut [usize],
    placement: &Placement3D,
) -> usize {
    for (index, demand) in problem.demands.iter().enumerate() {
        if remaining_by_demand[index] > 0 && placement_matches_demand(placement, demand) {
            return index;
        }
    }
    panic!("placement does not match a remaining demand: {placement:?}");
}

fn matching_unplaced_demand_index(
    problem: &ThreeDProblem,
    remaining_by_demand: &[usize],
    item: &BoxDemand3D,
) -> usize {
    for (index, demand) in problem.demands.iter().enumerate() {
        if remaining_by_demand[index] >= item.quantity
            && demand.name == item.name
            && demand.width == item.width
            && demand.height == item.height
            && demand.depth == item.depth
            && demand.allowed_rotations == item.allowed_rotations
        {
            return index;
        }
    }
    panic!("unplaced item does not match a remaining demand: {item:?}");
}

fn placement_matches_demand(placement: &Placement3D, demand: &BoxDemand3D) -> bool {
    if placement.name != demand.name || !demand.allowed_rotations.contains(placement.rotation) {
        return false;
    }
    let (width, height, depth) =
        placement.rotation.apply(demand.width, demand.height, demand.depth);
    (placement.width, placement.height, placement.depth) == (width, height, depth)
}

fn assert_non_overlapping(placements: &[Placement3D]) {
    for (left_index, left) in placements.iter().enumerate() {
        for right in placements.iter().skip(left_index + 1) {
            let separated = left.x.saturating_add(left.width) <= right.x
                || right.x.saturating_add(right.width) <= left.x
                || left.y.saturating_add(left.height) <= right.y
                || right.y.saturating_add(right.height) <= left.y
                || left.z.saturating_add(left.depth) <= right.z
                || right.z.saturating_add(right.depth) <= left.z;
            assert!(separated, "3D placements overlap: {left:?} vs {right:?}");
        }
    }
}

fn assert_bin_requirements(
    problem: &ThreeDProblem,
    solution: &ThreeDSolution,
    layout_counts: &[usize],
) {
    if problem.bins.iter().all(|bin| bin.quantity.is_none()) {
        assert!(
            solution.bin_requirements.is_empty(),
            "uncapped problems should not report bin requirements",
        );
        return;
    }

    assert_eq!(
        solution.bin_requirements.len(),
        problem.bins.len(),
        "capped problems should report one requirement per bin type",
    );
    for (index, requirement) in solution.bin_requirements.iter().enumerate() {
        let bin = &problem.bins[index];
        assert_eq!(requirement.bin_name, bin.name);
        assert_eq!(requirement.bin_width, bin.width);
        assert_eq!(requirement.bin_height, bin.height);
        assert_eq!(requirement.bin_depth, bin.depth);
        assert_eq!(requirement.cost, bin.cost);
        assert_eq!(requirement.available_quantity, bin.quantity);
        assert_eq!(requirement.used_quantity, layout_counts[index]);
        let expected_additional =
            bin.quantity.map(|cap| requirement.required_quantity.saturating_sub(cap)).unwrap_or(0);
        assert_eq!(requirement.additional_quantity_needed, expected_additional);
    }
}

fn volume(width: u32, height: u32, depth: u32) -> u64 {
    u64::from(width) * u64::from(height) * u64::from(depth)
}

fn assert_close_enough(lhs: f64, rhs: f64, term_count: usize, what: &str) {
    let scale = lhs.abs().max(rhs.abs()).max(1.0);
    let tolerance = (term_count.max(1) as f64) * 1e-9 * scale;
    assert!((lhs - rhs).abs() <= tolerance, "{what}: lhs={lhs} rhs={rhs} tolerance={tolerance}");
}

fn is_expected_error(error: &BinPackingError) -> bool {
    matches!(
        error,
        BinPackingError::InvalidInput(_)
            | BinPackingError::Unsupported(_)
            | BinPackingError::Infeasible3D { .. }
    )
}

fuzz_target!(|data: &[u8]| {
    let mut unstructured = Unstructured::new(data);
    let Ok(input) = ThreeDModuleSeed::arbitrary(&mut unstructured) else {
        return;
    };

    let algorithm = map_algorithm(input.algorithm);
    let problem = build_problem(&input, algorithm);
    let options = ThreeDOptions {
        algorithm,
        seed: Some(input.seed),
        multistart_runs: usize::from(input.multistart_runs.max(1)).min(5),
        improvement_rounds: usize::from(input.improvement_rounds).min(8),
        beam_width: usize::from(input.beam_width.max(1)).min(8),
        branch_and_bound_node_limit: usize::from(input.branch_and_bound_node_limit).max(1).min(512),
        guillotine_required: input.guillotine_required,
        ..ThreeDOptions::default()
    };

    match solve_3d(problem.clone(), options) {
        Ok(solution) => assert_three_d_solution(&problem, &solution),
        Err(error) => assert!(is_expected_error(&error), "unexpected 3D error: {error:?}"),
    }
});
