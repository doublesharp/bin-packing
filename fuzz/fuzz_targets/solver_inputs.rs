#![no_main]

use std::collections::BTreeMap;

use arbitrary::{Arbitrary, Unstructured};
use bin_packing::BinPackingError;
use bin_packing::one_d::{
    CutDemand1D, OneDAlgorithm, OneDOptions, OneDProblem, OneDSolution, Stock1D, solve_1d,
};
use bin_packing::three_d::{
    Bin3D, BoxDemand3D, RotationMask3D, ThreeDAlgorithm, ThreeDOptions, ThreeDProblem,
    ThreeDSolution, solve_3d,
};
use bin_packing::cut_plan::CutPlanError;
use bin_packing::two_d::{
    Placement2D, RectDemand2D, Sheet2D, TwoDAlgorithm, TwoDOptions, TwoDProblem, TwoDSolution,
    solve_2d,
};
use bin_packing::two_d::cut_plan::{CutPlanOptions2D, CutPlanPreset2D, plan_cuts};
use libfuzzer_sys::fuzz_target;

// Keep problem sizes small so the beam search and exact backends stay fast enough
// for libfuzzer to iterate on. These caps mirror what the integration tests use.
const MAX_STOCK_TYPES: usize = 3;
const MAX_SHEET_TYPES: usize = 3;
const MAX_ONE_D_DEMANDS: usize = 8;
const MAX_TWO_D_DEMANDS: usize = 8;
const MAX_DIMENSION_1D: u32 = 256;
const MAX_DIMENSION_2D: u32 = 96;
const MAX_QUANTITY: usize = 4;
const MAX_KERF: u32 = 3;
const MAX_TRIM: u32 = 6;
const MAX_AVAILABLE: usize = 6;
const MAX_BIN_TYPES: usize = 3;
const MAX_THREE_D_DEMANDS: usize = 6;
const MAX_DIMENSION_3D_FUZZ: u32 = 64;

// ---------------------------------------------------------------------------
// Input types the fuzzer draws from `Unstructured`.
// ---------------------------------------------------------------------------

#[derive(Debug, Arbitrary)]
struct StockSeed {
    length: u16,
    kerf: u8,
    trim: u8,
    cost_units: u8,
    available: Option<u8>,
}

#[derive(Debug, Arbitrary)]
struct SheetSeed {
    width: u16,
    height: u16,
    cost_units: u8,
    quantity: Option<u8>,
    kerf: u8,
    edge_kerf_relief: bool,
}

#[derive(Debug, Arbitrary)]
struct OneDDemandSeed {
    length: u16,
    quantity: u8,
}

#[derive(Debug, Arbitrary)]
struct TwoDDemandSeed {
    width: u16,
    height: u16,
    quantity: u8,
    can_rotate: bool,
}

#[derive(Debug, Arbitrary)]
struct OneDSeed {
    stock: Vec<StockSeed>,
    demands: Vec<OneDDemandSeed>,
    algorithm: u8,
    seed: u64,
}

#[derive(Debug, Arbitrary)]
struct TwoDSeed {
    sheets: Vec<SheetSeed>,
    demands: Vec<TwoDDemandSeed>,
    algorithm: u8,
    seed: u64,
    guillotine_required: bool,
    beam_width: u8,
    multistart_runs: u8,
    min_usable_side: u8,
    cut_plan_preset: u8,
}

#[derive(Debug, Arbitrary)]
struct BinSeed {
    width: u16,
    height: u16,
    depth: u16,
    cost_units: u8,
    quantity: Option<u8>,
}

#[derive(Debug, Arbitrary)]
struct ThreeDDemandSeed {
    width: u16,
    height: u16,
    depth: u16,
    quantity: u8,
}

#[derive(Debug, Arbitrary)]
struct ThreeDSeed {
    bins: Vec<BinSeed>,
    demands: Vec<ThreeDDemandSeed>,
    algorithm: u8,
    seed: u64,
    multistart_runs: u8,
}

#[derive(Debug, Arbitrary)]
struct SolverInput {
    one_d: OneDSeed,
    two_d: TwoDSeed,
    three_d: ThreeDSeed,
}

// ---------------------------------------------------------------------------
// Algorithm maps — keep these in sync with the enums in bin_packing.
// ---------------------------------------------------------------------------

fn map_one_d_algorithm(value: u8) -> OneDAlgorithm {
    match value % 5 {
        0 => OneDAlgorithm::Auto,
        1 => OneDAlgorithm::FirstFitDecreasing,
        2 => OneDAlgorithm::BestFitDecreasing,
        3 => OneDAlgorithm::LocalSearch,
        _ => OneDAlgorithm::ColumnGeneration,
    }
}

fn map_two_d_algorithm(value: u8) -> TwoDAlgorithm {
    // 19 variants total in TwoDAlgorithm — every branch must be reachable so the
    // fuzzer exercises the full dispatch table.
    match value % 19 {
        0 => TwoDAlgorithm::Auto,
        1 => TwoDAlgorithm::MaxRects,
        2 => TwoDAlgorithm::MaxRectsBestShortSideFit,
        3 => TwoDAlgorithm::MaxRectsBestLongSideFit,
        4 => TwoDAlgorithm::MaxRectsBottomLeft,
        5 => TwoDAlgorithm::MaxRectsContactPoint,
        6 => TwoDAlgorithm::Skyline,
        7 => TwoDAlgorithm::SkylineMinWaste,
        8 => TwoDAlgorithm::Guillotine,
        9 => TwoDAlgorithm::GuillotineBestShortSideFit,
        10 => TwoDAlgorithm::GuillotineBestLongSideFit,
        11 => TwoDAlgorithm::GuillotineShorterLeftoverAxis,
        12 => TwoDAlgorithm::GuillotineLongerLeftoverAxis,
        13 => TwoDAlgorithm::GuillotineMinAreaSplit,
        14 => TwoDAlgorithm::GuillotineMaxAreaSplit,
        15 => TwoDAlgorithm::NextFitDecreasingHeight,
        16 => TwoDAlgorithm::FirstFitDecreasingHeight,
        17 => TwoDAlgorithm::BestFitDecreasingHeight,
        _ => TwoDAlgorithm::MultiStart,
    }
}

fn map_cut_plan_preset(value: u8) -> CutPlanPreset2D {
    match value % 3 {
        0 => CutPlanPreset2D::TableSaw,
        1 => CutPlanPreset2D::PanelSaw,
        _ => CutPlanPreset2D::CncRouter,
    }
}

fn map_three_d_algorithm(value: u8) -> ThreeDAlgorithm {
    // 28 non-BranchAndBound variants — BranchAndBound is excluded because it
    // can be very slow on fuzz inputs without the size guards in auto mode.
    match value % 28 {
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
        26 => ThreeDAlgorithm::LocalSearch,
        _ => ThreeDAlgorithm::Grasp,
    }
}

// ---------------------------------------------------------------------------
// Problem builders — clamp values so the solver never sees infeasible garbage
// but still exercises interesting edge cases (multi-stock, inventory caps,
// rotation, shortfalls).
// ---------------------------------------------------------------------------

fn build_one_d_problem(seed: OneDSeed) -> OneDProblem {
    let mut stock = seed
        .stock
        .into_iter()
        .take(MAX_STOCK_TYPES)
        .enumerate()
        .map(|(index, stock_seed)| build_stock(index, stock_seed))
        .collect::<Vec<_>>();

    if stock.is_empty() {
        stock.push(Stock1D {
            name: "stock_0".to_string(),
            length: 32,
            kerf: 0,
            trim: 0,
            cost: 1.0,
            available: None,
        });
    }

    // Every demand must fit at least one stock type, otherwise `solve_1d` errors
    // out with `Infeasible1D`. We clamp each demand length to the smallest usable
    // stock length so the solver has something to do.
    let min_usable =
        stock.iter().map(|stock| stock.length.saturating_sub(stock.trim)).min().unwrap_or(8).max(1);

    let demands = seed
        .demands
        .into_iter()
        .take(MAX_ONE_D_DEMANDS)
        .enumerate()
        .map(|(index, demand)| CutDemand1D {
            name: format!("cut_{index}"),
            length: u32::from(demand.length.max(1)).min(min_usable),
            quantity: usize::from(demand.quantity % (MAX_QUANTITY as u8)).max(1),
        })
        .collect::<Vec<_>>();

    let demands = if demands.is_empty() {
        vec![CutDemand1D { name: "cut_0".to_string(), length: min_usable.min(8), quantity: 1 }]
    } else {
        demands
    };

    OneDProblem { stock, demands }
}

fn build_stock(index: usize, seed: StockSeed) -> Stock1D {
    // Length is drawn from u16 but clamped to the 1D budget so that even the
    // largest stock fits into the solver's u32 arithmetic without saturation.
    let length = u32::from(seed.length).clamp(16, MAX_DIMENSION_1D);
    let trim = u32::from(seed.trim % ((MAX_TRIM + 1) as u8)).min(length.saturating_sub(1));
    let kerf = u32::from(seed.kerf) % (MAX_KERF + 1);
    // Cost is deterministic, finite, positive — NaN/negative are rejected by
    // TwoDProblem::validate, so we map seed.cost_units into a clean range.
    let cost = 1.0 + f64::from(seed.cost_units % 8);
    let available = seed.available.map(|raw| usize::from(raw % ((MAX_AVAILABLE + 1) as u8)).max(1));

    Stock1D { name: format!("stock_{index}"), length, kerf, trim, cost, available }
}

fn build_two_d_problem(seed: &TwoDSeed) -> TwoDProblem {
    let mut sheets = seed
        .sheets
        .iter()
        .take(MAX_SHEET_TYPES)
        .enumerate()
        .map(|(index, sheet_seed)| build_sheet(index, sheet_seed))
        .collect::<Vec<_>>();

    if sheets.is_empty() {
        sheets.push(Sheet2D {
            name: "sheet_0".to_string(),
            width: 32,
            height: 32,
            cost: 1.0,
            quantity: None,
            kerf: 0,
            edge_kerf_relief: false,
        });
    }

    // Clamp demand dimensions so every rectangle fits the smallest sheet (after
    // accounting for rotation). This keeps the solver away from hard infeasibility
    // which short-circuits the interesting code paths.
    let (min_width, min_height) = sheets
        .iter()
        .map(|sheet| (sheet.width, sheet.height))
        .fold((u32::MAX, u32::MAX), |(min_w, min_h), (w, h)| (min_w.min(w), min_h.min(h)));
    // For rotatable demands we clamp both sides to `min(min_width, min_height)`
    // so that EITHER orientation fits the smallest sheet's width and height.
    // Clamping width to `min_width` alone allows a demand to be
    // `min_width × min_height` which, when rotated, becomes
    // `min_height × min_width` — that rotated orientation cannot legally
    // fit a sheet of exactly `min_width × min_height` unless both axes
    // can accommodate the swapped dims.
    let rotatable_side_cap = min_width.min(min_height);

    let demands = seed
        .demands
        .iter()
        .take(MAX_TWO_D_DEMANDS)
        .enumerate()
        .map(|(index, demand)| {
            let raw_width = u32::from(demand.width.max(1));
            let raw_height = u32::from(demand.height.max(1));
            // Force the rectangle into the bounding box that fits every sheet so
            // it's always placeable in at least one orientation.
            let (width, height) = if demand.can_rotate {
                (raw_width.min(rotatable_side_cap), raw_height.min(rotatable_side_cap))
            } else {
                (raw_width.min(min_width), raw_height.min(min_height))
            };
            RectDemand2D {
                name: format!("rect_{index}"),
                width,
                height,
                quantity: usize::from(demand.quantity % (MAX_QUANTITY as u8)).max(1),
                can_rotate: demand.can_rotate,
            }
        })
        .collect::<Vec<_>>();

    let demands = if demands.is_empty() {
        vec![RectDemand2D {
            name: "rect_0".to_string(),
            width: min_width.clamp(1, 8),
            height: min_height.clamp(1, 8),
            quantity: 1,
            can_rotate: true,
        }]
    } else {
        demands
    };

    TwoDProblem { sheets, demands }
}

fn build_sheet(index: usize, seed: &SheetSeed) -> Sheet2D {
    let width = u32::from(seed.width).clamp(16, MAX_DIMENSION_2D);
    let height = u32::from(seed.height).clamp(16, MAX_DIMENSION_2D);
    let cost = 1.0 + f64::from(seed.cost_units % 8);
    let quantity = seed.quantity.map(|raw| usize::from(raw % ((MAX_AVAILABLE + 1) as u8)).max(1));

    // Clamp kerf to satisfy Sheet2D validation: kerf * 2 < min(width, height).
    // The validator rejects >= min_side / 2, so cap at (min_side / 2) - 1. Cap
    // further at a small constant to keep fuzzer inputs realistic and keep
    // the free-space shrink per placement bounded.
    let min_side = width.min(height);
    let max_kerf = min_side.saturating_sub(1) / 2;
    let kerf = (u32::from(seed.kerf) % 8).min(max_kerf);

    Sheet2D {
        name: format!("sheet_{index}"),
        width,
        height,
        cost,
        quantity,
        kerf,
        edge_kerf_relief: seed.edge_kerf_relief,
    }
}

fn build_bin(index: usize, seed: BinSeed) -> Bin3D {
    let width = u32::from(seed.width).clamp(16, MAX_DIMENSION_3D_FUZZ);
    let height = u32::from(seed.height).clamp(16, MAX_DIMENSION_3D_FUZZ);
    let depth = u32::from(seed.depth).clamp(16, MAX_DIMENSION_3D_FUZZ);
    let cost = 1.0 + f64::from(seed.cost_units % 8);
    let quantity = seed.quantity.map(|raw| usize::from(raw % ((MAX_AVAILABLE + 1) as u8)).max(1));
    Bin3D { name: format!("bin_{index}"), width, height, depth, cost, quantity }
}

fn build_three_d_problem(seed: ThreeDSeed) -> ThreeDProblem {
    let mut bins = seed
        .bins
        .into_iter()
        .take(MAX_BIN_TYPES)
        .enumerate()
        .map(|(i, b)| build_bin(i, b))
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

    let min_dim = bins.iter().flat_map(|b| [b.width, b.height, b.depth]).min().unwrap_or(32);

    let demands = seed
        .demands
        .into_iter()
        .take(MAX_THREE_D_DEMANDS)
        .enumerate()
        .map(|(i, d)| BoxDemand3D {
            name: format!("item_{i}"),
            width: u32::from(d.width.max(1)).min(min_dim),
            height: u32::from(d.height.max(1)).min(min_dim),
            depth: u32::from(d.depth.max(1)).min(min_dim),
            quantity: usize::from(d.quantity % (MAX_QUANTITY as u8)).max(1),
            allowed_rotations: RotationMask3D::ALL,
        })
        .collect::<Vec<_>>();

    let demands = if demands.is_empty() {
        vec![BoxDemand3D {
            name: "item_0".to_string(),
            width: min_dim.clamp(1, 8),
            height: min_dim.clamp(1, 8),
            depth: min_dim.clamp(1, 8),
            quantity: 1,
            allowed_rotations: RotationMask3D::ALL,
        }]
    } else {
        demands
    };

    ThreeDProblem { bins, demands }
}

// ---------------------------------------------------------------------------
// Invariant checks — mirror the structure of the integration tests so bugs
// caught in fuzzing are easy to reproduce.
// ---------------------------------------------------------------------------

fn assert_one_d_invariants(problem: &OneDProblem, solution: &OneDSolution) {
    // Every stock name in a layout must correspond to a declared stock entry.
    let stock_lookup =
        problem.stock.iter().map(|stock| (stock.name.clone(), stock)).collect::<BTreeMap<_, _>>();

    // Track how many of each (name, length) pair are still "owed" to demands.
    let mut remaining_counts = BTreeMap::<(String, u32), usize>::new();
    for demand in &problem.demands {
        remaining_counts.insert((demand.name.clone(), demand.length), demand.quantity);
    }

    let mut total_waste = 0_u64;
    let mut layout_counts_by_stock = BTreeMap::<String, usize>::new();

    for layout in &solution.layouts {
        let stock = stock_lookup
            .get(&layout.stock_name)
            .expect("layout should reference a declared stock type");

        // Kerf accounting: the used length is the sum of cut lengths plus kerf
        // for every cut after the first. Use saturating arithmetic to match the
        // library and to survive any pathologically large fuzz inputs.
        let cuts_sum = layout.cuts.iter().map(|cut| cut.length).fold(0_u32, u32::saturating_add);
        let kerf_count = u32::try_from(layout.cuts.len().saturating_sub(1)).unwrap_or(u32::MAX);
        let expected_used = cuts_sum.saturating_add(stock.kerf.saturating_mul(kerf_count));

        let usable = stock.length.saturating_sub(stock.trim);
        assert!(
            layout.used_length <= usable,
            "used length {} exceeds usable {} on stock {}",
            layout.used_length,
            usable,
            layout.stock_name,
        );
        assert_eq!(
            layout.used_length, expected_used,
            "used length mismatch on stock {}",
            layout.stock_name
        );
        assert_eq!(
            layout.remaining_length,
            usable.saturating_sub(layout.used_length),
            "remaining length mismatch on stock {}",
            layout.stock_name,
        );
        assert_eq!(layout.waste, layout.remaining_length);

        for cut in &layout.cuts {
            let key = (cut.name.clone(), cut.length);
            let entry = remaining_counts
                .get_mut(&key)
                .expect("placed cut should correspond to a declared demand");
            assert!(*entry > 0, "cut was placed more times than demanded");
            *entry -= 1;
        }

        total_waste = total_waste.saturating_add(u64::from(layout.waste));
        *layout_counts_by_stock.entry(layout.stock_name.clone()).or_insert(0) += 1;
    }

    for cut in &solution.unplaced {
        let key = (cut.name.clone(), cut.length);
        let entry = remaining_counts
            .get_mut(&key)
            .expect("unplaced cut should correspond to a declared demand");
        assert!(*entry > 0, "cut was returned unplaced more times than demanded");
        *entry -= 1;
    }

    assert!(
        remaining_counts.values().all(|count| *count == 0),
        "not every demand was accounted for: {remaining_counts:?}"
    );
    assert_eq!(solution.layouts.len(), solution.stock_count);
    assert_eq!(solution.total_waste, total_waste);
    assert!(solution.lower_bound.is_none_or(|bound| bound >= 0.0));

    // Inventory cap: if a stock type has `available: Some(n)`, the solver must
    // never consume more than n layouts of that stock.
    for stock in &problem.stock {
        if let Some(cap) = stock.available {
            let used = layout_counts_by_stock.get(&stock.name).copied().unwrap_or(0);
            assert!(used <= cap, "stock {} used {} times but cap is {}", stock.name, used, cap);
        }
    }

    // Stock requirements report: when present, each entry must describe a real
    // stock type and its `used_quantity` must match the layout count, with
    // `additional_quantity_needed` equal to the shortfall against `available`.
    for requirement in &solution.stock_requirements {
        let stock = stock_lookup
            .get(&requirement.stock_name)
            .expect("stock requirement should reference a declared stock");
        assert_eq!(requirement.stock_length, stock.length);
        assert_eq!(requirement.usable_length, stock.length.saturating_sub(stock.trim));
        assert_eq!(requirement.available_quantity, stock.available);
        let actual_used = layout_counts_by_stock.get(&requirement.stock_name).copied().unwrap_or(0);
        assert_eq!(
            requirement.used_quantity, actual_used,
            "stock requirement used_quantity mismatch for {}",
            requirement.stock_name
        );
        // NOTE: we cannot assert `required_quantity >= used_quantity`. The
        // library fills `required_quantity` from a *relaxed* (unlimited
        // inventory) auto solve, which may pick a completely different stock
        // mix than the constrained solve. In that case a stock type can have
        // `used_quantity > 0` but `required_quantity == 0` because the relaxed
        // optimal packing avoids it entirely. Only the structural shape of
        // `additional_quantity_needed` is invariant.
        let expected_additional = match stock.available {
            Some(cap) => requirement.required_quantity.saturating_sub(cap),
            None => 0,
        };
        assert_eq!(
            requirement.additional_quantity_needed, expected_additional,
            "additional_quantity_needed mismatch for {}",
            requirement.stock_name
        );
    }

    // Total cost must equal the sum of per-layout costs within a generous
    // floating-point tolerance. We allow `layouts.len()` ULPs of slack since
    // f64 addition isn't associative and the library may sum in a different
    // order than we walk here.
    assert_close_enough(
        solution.total_cost,
        solution.layouts.iter().map(|layout| layout.cost).sum::<f64>(),
        solution.layouts.len(),
        "1D total_cost vs summed layout cost",
    );
}

/// Relative + absolute tolerance for float equality between two sums. The
/// tolerance grows linearly with the number of terms summed to absorb
/// non-associativity in f64 addition.
fn assert_close_enough(lhs: f64, rhs: f64, term_count: usize, what: &str) {
    let scale = lhs.abs().max(rhs.abs()).max(1.0);
    let tolerance = (term_count.max(1) as f64) * 1e-9 * scale;
    assert!((lhs - rhs).abs() <= tolerance, "{what}: lhs={lhs} rhs={rhs} tolerance={tolerance}");
}

fn assert_placements_non_overlapping(placements: &[Placement2D]) {
    for (left_index, left) in placements.iter().enumerate() {
        for right in placements.iter().skip(left_index + 1) {
            // Use saturating arithmetic — fuzz can supply dimensions that would
            // overflow naive addition if the clamps above ever regressed.
            let left_right = left.x.saturating_add(left.width);
            let left_bottom = left.y.saturating_add(left.height);
            let right_right = right.x.saturating_add(right.width);
            let right_bottom = right.y.saturating_add(right.height);
            let separated = left_right <= right.x
                || right_right <= left.x
                || left_bottom <= right.y
                || right_bottom <= left.y;
            assert!(separated, "placements overlap: {left:?} vs {right:?}");
        }
    }
}

fn assert_two_d_invariants(problem: &TwoDProblem, solution: &TwoDSolution) {
    let sheet_lookup =
        problem.sheets.iter().map(|sheet| (sheet.name.clone(), sheet)).collect::<BTreeMap<_, _>>();

    // Key demands by (name, width, height, can_rotate) — the integration test
    // uses the same shape so matching logic stays identical.
    let mut remaining = BTreeMap::<(String, u32, u32, bool), usize>::new();
    for demand in &problem.demands {
        remaining.insert(
            (demand.name.clone(), demand.width, demand.height, demand.can_rotate),
            demand.quantity,
        );
    }

    let mut total_waste = 0_u64;
    let mut layout_counts_by_sheet = BTreeMap::<String, usize>::new();

    for layout in &solution.layouts {
        let sheet =
            sheet_lookup.get(&layout.sheet_name).expect("layout should reference a declared sheet");
        let sheet_area = u64::from(sheet.width) * u64::from(sheet.height);

        assert_placements_non_overlapping(&layout.placements);

        let mut used_area = 0_u64;
        // Under edge_kerf_relief, the trailing placement may extend up to
        // one kerf past the sheet's right and bottom edges. Without the
        // flag, the strict bound applies.
        let max_x = if sheet.edge_kerf_relief {
            sheet.width.saturating_add(sheet.kerf)
        } else {
            sheet.width
        };
        let max_y = if sheet.edge_kerf_relief {
            sheet.height.saturating_add(sheet.kerf)
        } else {
            sheet.height
        };
        for placement in &layout.placements {
            let placement_right = placement.x.saturating_add(placement.width);
            let placement_bottom = placement.y.saturating_add(placement.height);
            assert!(
                placement_right <= max_x,
                "placement extends past effective sheet width: {placement:?} on sheet {} (max_x={max_x})",
                sheet.name
            );
            assert!(
                placement_bottom <= max_y,
                "placement extends past effective sheet height: {placement:?} on sheet {} (max_y={max_y})",
                sheet.name
            );
            // Parts themselves must always fit within sheet dims.
            assert!(
                placement.width <= sheet.width && placement.height <= sheet.height,
                "placement dims exceed sheet: {placement:?} on sheet {}",
                sheet.name
            );
            assert!(
                placement.width > 0 && placement.height > 0,
                "placement has zero dimension: {placement:?}"
            );

            let demand = problem
                .demands
                .iter()
                .find(|demand| {
                    demand.name == placement.name
                        && ((demand.width == placement.width && demand.height == placement.height)
                            || (demand.can_rotate
                                && demand.width == placement.height
                                && demand.height == placement.width))
                })
                .expect("placement should correspond to a declared demand");

            // Honor the rotation flag: a non-rotatable demand must never produce
            // a rotated placement.
            if !demand.can_rotate {
                assert!(!placement.rotated, "non-rotatable demand was rotated: {placement:?}");
                assert_eq!(placement.width, demand.width);
                assert_eq!(placement.height, demand.height);
            }

            let key = (demand.name.clone(), demand.width, demand.height, demand.can_rotate);
            let entry =
                remaining.get_mut(&key).expect("declared demand should have a remaining counter");
            assert!(*entry > 0, "placement exceeds requested quantity");
            *entry -= 1;
            // Sum the on-sheet portion of each placement so this matches
            // `from_layouts.used_area`, which clips to sheet bounds when
            // `edge_kerf_relief` is on.
            let on_sheet_w = placement
                .x
                .saturating_add(placement.width)
                .min(sheet.width)
                .saturating_sub(placement.x);
            let on_sheet_h = placement
                .y
                .saturating_add(placement.height)
                .min(sheet.height)
                .saturating_sub(placement.y);
            used_area = used_area
                .saturating_add(u64::from(on_sheet_w) * u64::from(on_sheet_h));
        }

        assert_eq!(layout.used_area, used_area, "layout used_area mismatch");
        assert_eq!(
            layout.waste_area,
            sheet_area.saturating_sub(used_area),
            "layout waste_area mismatch"
        );
        assert_eq!(layout.width, sheet.width, "layout cached width mismatch");
        assert_eq!(layout.height, sheet.height, "layout cached height mismatch");

        // Kerf edge-gap invariant: when the sheet declares a non-zero kerf,
        // edge-adjacent placements must be separated by at least `kerf` along
        // the shared axis. Overlap on both axes is already caught by
        // `assert_placements_non_overlapping` above.
        if sheet.kerf > 0 {
            let kerf = sheet.kerf;
            let placements = &layout.placements;
            for (i, a) in placements.iter().enumerate() {
                let a_right = a.x.saturating_add(a.width);
                let a_bottom = a.y.saturating_add(a.height);
                for b in placements.iter().skip(i + 1) {
                    let b_right = b.x.saturating_add(b.width);
                    let b_bottom = b.y.saturating_add(b.height);

                    let y_overlap = a.y.max(b.y) < a_bottom.min(b_bottom);
                    if y_overlap {
                        let gap = if a_right <= b.x {
                            b.x - a_right
                        } else if b_right <= a.x {
                            a.x - b_right
                        } else {
                            0
                        };
                        assert!(
                            gap >= kerf,
                            "x-adjacent placements must be >= kerf={kerf} apart, got {gap}: {a:?} vs {b:?}"
                        );
                    }

                    let x_overlap = a.x.max(b.x) < a_right.min(b_right);
                    if x_overlap {
                        let gap = if a_bottom <= b.y {
                            b.y - a_bottom
                        } else if b_bottom <= a.y {
                            a.y - b_bottom
                        } else {
                            0
                        };
                        assert!(
                            gap >= kerf,
                            "y-adjacent placements must be >= kerf={kerf} apart, got {gap}: {a:?} vs {b:?}"
                        );
                    }
                }
            }
        }

        total_waste = total_waste.saturating_add(layout.waste_area);
        *layout_counts_by_sheet.entry(layout.sheet_name.clone()).or_insert(0) += 1;
    }

    for item in &solution.unplaced {
        let key = (item.name.clone(), item.width, item.height, item.can_rotate);
        let entry =
            remaining.get_mut(&key).expect("unplaced item should correspond to a declared demand");
        assert!(*entry >= item.quantity, "unplaced item exceeds requested quantity");
        *entry -= item.quantity;
    }

    assert!(
        remaining.values().all(|count| *count == 0),
        "not every demand was accounted for: {remaining:?}"
    );
    assert_eq!(solution.layouts.len(), solution.sheet_count);
    assert_eq!(solution.total_waste_area, total_waste);
    assert_eq!(
        solution.total_kerf_area,
        solution.layouts.iter().map(|layout| layout.kerf_area).sum::<u64>(),
        "total_kerf_area must equal sum of per-layout kerf_area",
    );

    // Consolidation-metric invariants: per-layout largest-drop bounded by
    // waste, solution-level max aggregation matches per-layout maxes.
    for layout in &solution.layouts {
        assert!(
            layout.largest_usable_drop_area <= layout.waste_area,
            "largest_usable_drop_area {} exceeds waste_area {} on sheet {}",
            layout.largest_usable_drop_area,
            layout.waste_area,
            layout.sheet_name,
        );
    }
    let expected_max_drop =
        solution.layouts.iter().map(|l| l.largest_usable_drop_area).max().unwrap_or(0);
    assert_eq!(
        solution.max_usable_drop_area, expected_max_drop,
        "max_usable_drop_area must equal max of per-layout largest_usable_drop_area",
    );
    let expected_sum_sq = solution
        .layouts
        .iter()
        .map(|l| l.sum_sq_usable_drop_areas)
        .fold(0_u128, u128::saturating_add);
    assert_eq!(
        solution.total_sum_sq_usable_drop_areas, expected_sum_sq,
        "total_sum_sq_usable_drop_areas must equal saturating sum of per-layout values",
    );

    // Sheet inventory cap: if a sheet type has `quantity: Some(n)`, the solver
    // must never open more than n of that sheet.
    for sheet in &problem.sheets {
        if let Some(cap) = sheet.quantity {
            let used = layout_counts_by_sheet.get(&sheet.name).copied().unwrap_or(0);
            assert!(used <= cap, "sheet {} used {} times but cap is {}", sheet.name, used, cap);
        }
    }

    // Total cost must match the sum of per-layout costs within a generous
    // floating-point tolerance (see `assert_close_enough` for the rationale).
    assert_close_enough(
        solution.total_cost,
        solution.layouts.iter().map(|layout| layout.cost).sum::<f64>(),
        solution.layouts.len(),
        "2D total_cost vs summed layout cost",
    );
}

fn assert_three_d_invariants(problem: &ThreeDProblem, solution: &ThreeDSolution) {
    let bin_lookup = problem
        .bins
        .iter()
        .map(|b| (b.name.clone(), b))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut remaining = std::collections::BTreeMap::<(String, u32, u32, u32), usize>::new();
    for demand in &problem.demands {
        remaining.insert(
            (demand.name.clone(), demand.width, demand.height, demand.depth),
            demand.quantity,
        );
    }

    let mut total_waste = 0_u64;
    let mut layout_counts = std::collections::BTreeMap::<String, usize>::new();

    for layout in &solution.layouts {
        let bin = bin_lookup.get(&layout.bin_name).expect("layout bin must be declared");

        // Check placements non-overlapping.
        for (i, a) in layout.placements.iter().enumerate() {
            for b in layout.placements.iter().skip(i + 1) {
                let separated = a.x.saturating_add(a.width) <= b.x
                    || b.x.saturating_add(b.width) <= a.x
                    || a.y.saturating_add(a.height) <= b.y
                    || b.y.saturating_add(b.height) <= a.y
                    || a.z.saturating_add(a.depth) <= b.z
                    || b.z.saturating_add(b.depth) <= a.z;
                assert!(separated, "3D placements overlap: {a:?} vs {b:?}");
            }
        }

        let mut used_vol = 0_u64;
        for p in &layout.placements {
            assert!(p.x.saturating_add(p.width) <= bin.width, "x out of bounds: {p:?}");
            assert!(p.y.saturating_add(p.height) <= bin.height, "y out of bounds: {p:?}");
            assert!(p.z.saturating_add(p.depth) <= bin.depth, "z out of bounds: {p:?}");
            assert!(p.width > 0 && p.height > 0 && p.depth > 0, "zero-dim placement: {p:?}");

            let demand = problem
                .demands
                .iter()
                .find(|d| {
                    if d.name != p.name {
                        return false;
                    }
                    let (w, h, depth) = (d.width, d.height, d.depth);
                    [
                        (w, h, depth),
                        (w, depth, h),
                        (h, w, depth),
                        (h, depth, w),
                        (depth, w, h),
                        (depth, h, w),
                    ]
                    .iter()
                    .any(|&(pw, ph, pd)| pw == p.width && ph == p.height && pd == p.depth)
                })
                .expect("placement must match a demand");

            let key = (demand.name.clone(), demand.width, demand.height, demand.depth);
            let entry = remaining.get_mut(&key).expect("demand counter must exist");
            assert!(*entry > 0, "demand placed more times than demanded");
            *entry -= 1;
            used_vol = used_vol
                .saturating_add(u64::from(p.width) * u64::from(p.height) * u64::from(p.depth));
        }

        assert_eq!(layout.used_volume, used_vol, "used_volume mismatch");
        let bin_vol = u64::from(bin.width) * u64::from(bin.height) * u64::from(bin.depth);
        assert_eq!(layout.waste_volume, bin_vol.saturating_sub(used_vol), "waste_volume mismatch");
        total_waste = total_waste.saturating_add(layout.waste_volume);
        *layout_counts.entry(layout.bin_name.clone()).or_insert(0) += 1;
    }

    for item in &solution.unplaced {
        let key = (item.name.clone(), item.width, item.height, item.depth);
        let entry = remaining.get_mut(&key).expect("unplaced must match a demand");
        assert!(*entry >= item.quantity, "unplaced exceeds demand");
        *entry -= item.quantity;
    }

    assert!(remaining.values().all(|&c| c == 0), "demand not fully accounted: {remaining:?}");
    assert_eq!(solution.layouts.len(), solution.bin_count);
    assert_eq!(solution.total_waste_volume, total_waste);

    for bin in &problem.bins {
        if let Some(cap) = bin.quantity {
            let used = layout_counts.get(&bin.name).copied().unwrap_or(0);
            assert!(used <= cap, "bin {} exceeds cap: used={} cap={}", bin.name, used, cap);
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptable error returns.
// ---------------------------------------------------------------------------

/// The solver is allowed to return these error variants without it counting as
/// a fuzz finding. Everything else is treated as a bug.
fn is_expected_error(error: &BinPackingError) -> bool {
    matches!(
        error,
        BinPackingError::InvalidInput(_)
            | BinPackingError::Unsupported(_)
            | BinPackingError::Infeasible1D { .. }
            | BinPackingError::Infeasible2D { .. }
            | BinPackingError::Infeasible3D { .. }
    )
}

// ---------------------------------------------------------------------------
// Fuzz entry point.
// ---------------------------------------------------------------------------

fuzz_target!(|data: &[u8]| {
    let mut unstructured = Unstructured::new(data);
    let Ok(input) = SolverInput::arbitrary(&mut unstructured) else {
        return;
    };
    let SolverInput { one_d, two_d, three_d } = input;

    // --- 1D pass ---
    let one_d_algorithm = map_one_d_algorithm(one_d.algorithm);
    let one_d_seed = one_d.seed;
    let one_d_problem = build_one_d_problem(one_d);
    let one_d_options = OneDOptions {
        algorithm: one_d_algorithm,
        seed: Some(one_d_seed),
        ..OneDOptions::default()
    };

    match solve_1d(one_d_problem.clone(), one_d_options) {
        Ok(solution) => assert_one_d_invariants(&one_d_problem, &solution),
        Err(error) => assert!(is_expected_error(&error), "unexpected 1D error: {error:?}"),
    }

    // --- 2D pass ---
    let two_d_algorithm = map_two_d_algorithm(two_d.algorithm);
    let two_d_seed_value = two_d.seed;
    let guillotine_required = two_d.guillotine_required;
    let cut_plan_preset = map_cut_plan_preset(two_d.cut_plan_preset);
    // beam_width must be >= 1 or the solver spins in a degenerate loop — match
    // the `.max(1)` clamp used inside `solve_guillotine`.
    let beam_width = usize::from(two_d.beam_width.max(1)).min(8);
    let multistart_runs = usize::from(two_d.multistart_runs.max(1)).min(6);
    let two_d_problem = build_two_d_problem(&two_d);
    // Clamp min_usable_side to keep the threshold below the smallest sheet
    // side (validation allows up to kerf*2 < min_side; for consolidation we
    // just need it plausible and bounded).
    let min_sheet_side = two_d_problem
        .sheets
        .iter()
        .map(|s| s.width.min(s.height))
        .min()
        .unwrap_or(1);
    let min_usable_side =
        u32::from(two_d.min_usable_side % 16).min(min_sheet_side.saturating_sub(1));
    let two_d_options = TwoDOptions {
        algorithm: two_d_algorithm,
        seed: Some(two_d_seed_value),
        guillotine_required,
        beam_width,
        multistart_runs,
        min_usable_side,
    };

    match solve_2d(two_d_problem.clone(), two_d_options) {
        Ok(solution) => {
            assert_two_d_invariants(&two_d_problem, &solution);

            // Post-condition: cut plan must succeed (or fail with the only
            // expected error) and, when Ok, must produce a finite total cost.
            let cut_options =
                CutPlanOptions2D { preset: cut_plan_preset, ..Default::default() };
            match plan_cuts(&solution, &cut_options) {
                Ok(cut_plan) => {
                    assert!(
                        cut_plan.total_cost.is_finite(),
                        "cut_plan.total_cost must be finite, got {}",
                        cut_plan.total_cost
                    );
                }
                Err(CutPlanError::NonGuillotineNotCuttable { .. }) => {
                    // Only allowed for single-blade presets; CncRouter must
                    // always succeed.
                    assert!(
                        cut_plan_preset == CutPlanPreset2D::TableSaw
                            || cut_plan_preset == CutPlanPreset2D::PanelSaw,
                        "CncRouter should never return NonGuillotineNotCuttable"
                    );
                }
                Err(other) => {
                    panic!("unexpected cut-plan error: {other:?}");
                }
            }
        }
        Err(error) => assert!(is_expected_error(&error), "unexpected 2D error: {error:?}"),
    }

    // --- 3D pass ---
    let three_d_algorithm = map_three_d_algorithm(three_d.algorithm);
    let three_d_seed_value = three_d.seed;
    let multistart_runs_3d = usize::from(three_d.multistart_runs.max(1)).min(4);
    let three_d_problem = build_three_d_problem(three_d);
    let three_d_options = ThreeDOptions {
        algorithm: three_d_algorithm,
        seed: Some(three_d_seed_value),
        multistart_runs: multistart_runs_3d,
        ..ThreeDOptions::default()
    };

    match solve_3d(three_d_problem.clone(), three_d_options) {
        Ok(solution) => assert_three_d_invariants(&three_d_problem, &solution),
        Err(error) => assert!(is_expected_error(&error), "unexpected 3D error: {error:?}"),
    }
});
