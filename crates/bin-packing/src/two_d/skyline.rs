use std::cmp::Ordering;

use crate::Result;

use super::model::{
    ItemInstance2D, Placement2D, SolverMetrics2D, TwoDOptions, TwoDProblem, TwoDSolution,
};

#[derive(Debug, Clone)]
struct SkylineNode {
    x: u32,
    y: u32,
    width: u32,
}

#[derive(Debug, Clone)]
struct SheetState {
    stock_index: usize,
    nodes: Vec<SkylineNode>,
    placements: Vec<Placement2D>,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    sheet_index: usize,
    node_index: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    rotated: bool,
    top: u32,
    left: u32,
    waste: u64,
}

#[derive(Debug, Clone, Copy)]
enum SkylineStrategy {
    BottomLeft,
    MinWaste,
}

pub(super) fn solve_skyline(problem: &TwoDProblem, _options: &TwoDOptions) -> Result<TwoDSolution> {
    solve_with_strategy(problem, SkylineStrategy::BottomLeft, "skyline")
}

pub(super) fn solve_skyline_min_waste(
    problem: &TwoDProblem,
    _options: &TwoDOptions,
) -> Result<TwoDSolution> {
    solve_with_strategy(problem, SkylineStrategy::MinWaste, "skyline_min_waste")
}

fn solve_with_strategy(
    problem: &TwoDProblem,
    strategy: SkylineStrategy,
    algorithm: &str,
) -> Result<TwoDSolution> {
    let mut items = problem.expanded_items();
    items.sort_by(|left, right| {
        // Widen to u64 before multiplying — u32 * u32 can overflow at the
        // MAX_DIMENSION = 1 << 30 cap.
        let left_area = u64::from(left.width) * u64::from(left.height);
        let right_area = u64::from(right.width) * u64::from(right.height);
        right_area.cmp(&left_area)
    });

    let mut sheets = Vec::<SheetState>::new();
    let mut usage_counts = vec![0_usize; problem.sheets.len()];
    let mut unplaced = Vec::new();

    for item in items {
        if let Some(candidate) = choose_existing_candidate(problem, &sheets, &item, strategy) {
            place_candidate(&mut sheets[candidate.sheet_index], &item, candidate);
            continue;
        }

        if let Some(new_sheet) = choose_new_sheet(problem, &item, &usage_counts, strategy) {
            let sheet = &problem.sheets[new_sheet.stock_index];
            let mut state = SheetState {
                stock_index: new_sheet.stock_index,
                nodes: vec![SkylineNode { x: 0, y: 0, width: sheet.width }],
                placements: Vec::new(),
            };

            // A fresh sheet has a flat skyline at y == 0, so there is no trapped
            // waste under the first item — it sits flush in the bottom-left corner.
            let candidate = Candidate {
                sheet_index: 0,
                node_index: 0,
                x: 0,
                y: 0,
                width: new_sheet.width,
                height: new_sheet.height,
                rotated: new_sheet.rotated,
                top: new_sheet.height,
                left: 0,
                waste: 0,
            };

            place_candidate(&mut state, &item, candidate);
            sheets.push(state);
            usage_counts[new_sheet.stock_index] =
                usage_counts[new_sheet.stock_index].saturating_add(1);
        } else {
            unplaced.push(item);
        }
    }

    let layouts =
        sheets.into_iter().map(|sheet| (sheet.stock_index, sheet.placements)).collect::<Vec<_>>();

    Ok(TwoDSolution::from_layouts(
        algorithm,
        false,
        &problem.sheets,
        layouts,
        unplaced,
        SolverMetrics2D {
            iterations: 1,
            explored_states: 0,
            notes: vec![strategy.note().to_string()],
        },
    ))
}

fn choose_existing_candidate(
    problem: &TwoDProblem,
    sheets: &[SheetState],
    item: &ItemInstance2D,
    strategy: SkylineStrategy,
) -> Option<Candidate> {
    sheets
        .iter()
        .enumerate()
        .flat_map(|(sheet_index, sheet)| {
            sheet.nodes.iter().enumerate().flat_map(move |(node_index, node)| {
                item.orientations().filter_map(move |(width, height, rotated)| {
                    let fit = skyline_fit(problem, sheet, node_index, width, height)?;
                    Some(Candidate {
                        sheet_index,
                        node_index,
                        x: node.x,
                        y: fit.y,
                        width,
                        height,
                        rotated,
                        top: fit.y.saturating_add(height),
                        left: node.x,
                        waste: fit.wasted_area,
                    })
                })
            })
        })
        .min_by(|left, right| compare_candidates(strategy, left, right))
}

#[derive(Debug, Clone, Copy)]
struct NewSheetCandidate {
    stock_index: usize,
    width: u32,
    height: u32,
    rotated: bool,
    cost: f64,
}

fn choose_new_sheet(
    problem: &TwoDProblem,
    item: &ItemInstance2D,
    usage_counts: &[usize],
    strategy: SkylineStrategy,
) -> Option<NewSheetCandidate> {
    problem
        .sheets
        .iter()
        .enumerate()
        .filter(|(index, sheet)| {
            sheet.quantity.map(|quantity| usage_counts[*index] < quantity).unwrap_or(true)
        })
        .flat_map(|(stock_index, sheet)| {
            item.orientations()
                .filter(move |(width, height, _)| sheet.width >= *width && sheet.height >= *height)
                .map(move |(width, height, rotated)| NewSheetCandidate {
                    stock_index,
                    width,
                    height,
                    rotated,
                    cost: sheet.cost,
                })
        })
        .min_by(|left, right| {
            // A fresh sheet places the item flush in the bottom-left corner, so there
            // is no trapped waste under it (waste == 0), top == height, and left == 0.
            // Those invariants make both SkylineStrategy variants collapse to picking
            // the candidate with the smallest top, then smallest cost, then the
            // sheet's own dimensions as tie-breakers.
            strategy
                .compare(0, left.height, 0, 0, right.height, 0)
                .then_with(|| left.cost.total_cmp(&right.cost))
                .then_with(|| {
                    problem.sheets[left.stock_index]
                        .width
                        .cmp(&problem.sheets[right.stock_index].width)
                })
                .then_with(|| {
                    problem.sheets[left.stock_index]
                        .height
                        .cmp(&problem.sheets[right.stock_index].height)
                })
        })
}

#[derive(Debug, Clone, Copy)]
struct SkylineFit {
    /// Y coordinate where the item's bottom edge sits after fitting.
    y: u32,
    /// Wasted area trapped under the item: the sum of `gap_height * overlap_width`
    /// for every existing skyline node covered by the item's footprint.
    wasted_area: u64,
}

fn skyline_fit(
    problem: &TwoDProblem,
    sheet: &SheetState,
    node_index: usize,
    width: u32,
    height: u32,
) -> Option<SkylineFit> {
    let sheet_def = &problem.sheets[sheet.stock_index];
    let x = sheet.nodes[node_index].x;
    if x.saturating_add(width) > sheet_def.width {
        return None;
    }

    // First pass: determine the baseline y such that the item clears every node
    // whose horizontal extent overlaps the item's footprint.
    let mut width_left = width;
    let mut y = sheet.nodes[node_index].y;
    let mut index = node_index;

    while width_left > 0 {
        let node = &sheet.nodes[index];
        y = y.max(node.y);
        if y.saturating_add(height) > sheet_def.height {
            return None;
        }

        if node.width >= width_left {
            break;
        }

        width_left = width_left.saturating_sub(node.width);
        index += 1;
        if index >= sheet.nodes.len() {
            return None;
        }
    }

    // Second pass: now that y is known, accumulate the wasted area trapped under
    // the item for every node its footprint covers.
    let mut wasted_area = 0_u64;
    let mut width_left = width;
    let mut index = node_index;
    while width_left > 0 {
        let node = &sheet.nodes[index];
        let covered = node.width.min(width_left);
        let gap = y.saturating_sub(node.y);
        wasted_area = wasted_area.saturating_add(u64::from(covered) * u64::from(gap));
        if node.width >= width_left {
            break;
        }
        width_left = width_left.saturating_sub(node.width);
        index += 1;
        if index >= sheet.nodes.len() {
            break;
        }
    }

    Some(SkylineFit { y, wasted_area })
}

fn place_candidate(sheet: &mut SheetState, item: &ItemInstance2D, candidate: Candidate) {
    sheet.placements.push(Placement2D {
        name: item.name.clone(),
        x: candidate.x,
        y: candidate.y,
        width: candidate.width,
        height: candidate.height,
        rotated: candidate.rotated,
    });

    add_skyline_level(
        &mut sheet.nodes,
        candidate.node_index,
        candidate.x,
        candidate.y,
        candidate.width,
        candidate.height,
    );
}

fn add_skyline_level(
    nodes: &mut Vec<SkylineNode>,
    index: usize,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) {
    nodes.insert(index, SkylineNode { x, y: y.saturating_add(height), width });

    // Walk forward from index+1, computing how many nodes to drop entirely
    // and whether the next remaining node needs to be shrunk.
    let cutoff = nodes[index].x.saturating_add(nodes[index].width);
    let mut drop_end = index + 1;
    while drop_end < nodes.len() && nodes[drop_end].x < cutoff {
        let shrink = cutoff.saturating_sub(nodes[drop_end].x);
        if nodes[drop_end].width <= shrink {
            drop_end += 1;
        } else {
            nodes[drop_end].x = nodes[drop_end].x.saturating_add(shrink);
            nodes[drop_end].width = nodes[drop_end].width.saturating_sub(shrink);
            break;
        }
    }
    if drop_end > index + 1 {
        nodes.drain((index + 1)..drop_end);
    }

    merge_nodes(nodes);
}

fn merge_nodes(nodes: &mut Vec<SkylineNode>) {
    if nodes.len() < 2 {
        return;
    }
    let mut write = 0;
    for read in 1..nodes.len() {
        if nodes[write].y == nodes[read].y {
            nodes[write].width = nodes[write].width.saturating_add(nodes[read].width);
        } else {
            write += 1;
            if write != read {
                nodes[write] = nodes[read].clone();
            }
        }
    }
    nodes.truncate(write + 1);
}

fn compare_candidates(strategy: SkylineStrategy, left: &Candidate, right: &Candidate) -> Ordering {
    strategy
        .compare(left.waste, left.top, left.left, right.waste, right.top, right.left)
        .then_with(|| left.sheet_index.cmp(&right.sheet_index))
        .then_with(|| left.node_index.cmp(&right.node_index))
}

impl SkylineStrategy {
    fn compare(
        self,
        left_waste: u64,
        left_top: u32,
        left_left: u32,
        right_waste: u64,
        right_top: u32,
        right_left: u32,
    ) -> Ordering {
        match self {
            Self::BottomLeft => left_top
                .cmp(&right_top)
                .then_with(|| left_left.cmp(&right_left))
                .then_with(|| left_waste.cmp(&right_waste)),
            Self::MinWaste => left_waste
                .cmp(&right_waste)
                .then_with(|| left_top.cmp(&right_top))
                .then_with(|| left_left.cmp(&right_left)),
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::BottomLeft => "bottom-left skyline best-fit heuristic",
            Self::MinWaste => "minimum-waste skyline heuristic",
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::two_d::{RectDemand2D, Sheet2D, TwoDOptions, TwoDProblem};

    use super::{
        SheetState, SkylineNode, SkylineStrategy, skyline_fit, solve_skyline,
        solve_skyline_min_waste,
    };

    #[test]
    fn skyline_rotates_item_when_helpful() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 8,
                height: 6,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 6,
                height: 4,
                quantity: 1,
                can_rotate: true,
            }],
        };

        let solution = solve_skyline(&problem, &TwoDOptions::default()).expect("pack");
        assert_eq!(solution.sheet_count, 1);
        assert!(solution.unplaced.is_empty());
    }

    #[test]
    fn skyline_marks_items_unplaced_when_sheet_inventory_runs_out() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 6,
                height: 6,
                cost: 1.0,
                quantity: Some(1),
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 6,
                height: 6,
                quantity: 2,
                can_rotate: false,
            }],
        };

        let solution = solve_skyline(&problem, &TwoDOptions::default()).expect("pack");
        assert_eq!(solution.sheet_count, 1);
        assert_eq!(solution.unplaced.len(), 1);
    }

    #[test]
    fn skyline_fit_returns_none_when_width_runs_past_available_nodes() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 6,
                height: 6,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 1,
                height: 1,
                quantity: 1,
                can_rotate: false,
            }],
        };
        let sheet = SheetState {
            stock_index: 0,
            nodes: vec![SkylineNode { x: 0, y: 0, width: 2 }, SkylineNode { x: 2, y: 1, width: 2 }],
            placements: Vec::new(),
        };

        assert!(skyline_fit(&problem, &sheet, 0, 5, 1).is_none());
    }

    #[test]
    fn skyline_fit_reports_trapped_waste_under_item() {
        // A sheet with an uneven skyline: a low-left node at y=0 and a tall-right
        // node at y=3. Placing a 4-wide item across both baselines lifts the item
        // to y=3, trapping a 2x3 gap above the left node.
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 8,
                height: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![RectDemand2D {
                name: "panel".to_string(),
                width: 1,
                height: 1,
                quantity: 1,
                can_rotate: false,
            }],
        };
        let sheet = SheetState {
            stock_index: 0,
            nodes: vec![SkylineNode { x: 0, y: 0, width: 2 }, SkylineNode { x: 2, y: 3, width: 6 }],
            placements: Vec::new(),
        };

        let fit = skyline_fit(&problem, &sheet, 0, 4, 2).expect("fit should succeed");
        assert_eq!(fit.y, 3);
        // Wasted area: 2 wide * 3 gap over the left node, 2 wide * 0 gap over the right node.
        assert_eq!(fit.wasted_area, 6);

        // A fresh item sitting on a flat baseline has no trapped waste.
        let flat_sheet = SheetState {
            stock_index: 0,
            nodes: vec![SkylineNode { x: 0, y: 0, width: 8 }],
            placements: Vec::new(),
        };
        let flat_fit = skyline_fit(&problem, &flat_sheet, 0, 4, 2).expect("fit should succeed");
        assert_eq!(flat_fit.y, 0);
        assert_eq!(flat_fit.wasted_area, 0);
    }

    #[test]
    fn skyline_min_waste_variant_is_available() {
        let problem = TwoDProblem {
            sheets: vec![Sheet2D {
                name: "sheet".to_string(),
                width: 10,
                height: 10,
                cost: 1.0,
                quantity: None,
            }],
            demands: vec![
                RectDemand2D {
                    name: "A".to_string(),
                    width: 6,
                    height: 4,
                    quantity: 1,
                    can_rotate: false,
                },
                RectDemand2D {
                    name: "B".to_string(),
                    width: 4,
                    height: 6,
                    quantity: 1,
                    can_rotate: false,
                },
            ],
        };

        let solution = solve_skyline_min_waste(&problem, &TwoDOptions::default()).expect("pack");
        assert_eq!(solution.algorithm, "skyline_min_waste");
        assert!(solution.unplaced.is_empty());
    }

    /// Differential ranking: BottomLeft and MinWaste must pick different
    /// winners on crafted metrics. BottomLeft sorts by `(top, left, waste)`;
    /// MinWaste sorts by `(waste, top, left)`. Craft a pair where the lowest
    /// `top` has the highest `waste` and vice versa, so the two strategies
    /// invert their choice.
    #[test]
    fn skyline_strategy_compare_picks_per_strategy_primary_key() {
        use std::cmp::Ordering;

        // Candidate A: low top (good for BottomLeft) but high waste.
        // Candidate B: low waste (good for MinWaste) but high top.
        let a_waste = 100_u64;
        let a_top = 10_u32;
        let a_left = 0_u32;
        let b_waste = 5_u64;
        let b_top = 80_u32;
        let b_left = 0_u32;

        // BottomLeft: lower top wins → A.
        assert_eq!(
            SkylineStrategy::BottomLeft.compare(a_waste, a_top, a_left, b_waste, b_top, b_left),
            Ordering::Less
        );

        // MinWaste: lower waste wins → B.
        assert_eq!(
            SkylineStrategy::MinWaste.compare(a_waste, a_top, a_left, b_waste, b_top, b_left),
            Ordering::Greater
        );
    }

    /// `add_skyline_level` at index 0 is the "insert at the start" edge case
    /// that the drop-end/drain logic used to mishandle when I reviewed it.
    /// Verify it leaves the node list in a valid state (monotone x).
    #[test]
    fn add_skyline_level_at_index_zero_preserves_monotone_x() {
        let mut nodes =
            vec![SkylineNode { x: 0, y: 0, width: 4 }, SkylineNode { x: 4, y: 0, width: 6 }];
        // Simulate placing an item at x=0, y=0, w=2, h=5. The function inserts
        // a new node and then shrinks the first existing node from the left.
        super::add_skyline_level(&mut nodes, 0, 0, 0, 2, 5);

        // Monotone x: every node's x must be >= the previous node's x + width.
        for pair in nodes.windows(2) {
            let prev_right = pair[0].x + pair[0].width;
            assert!(pair[1].x >= prev_right, "skyline nodes must be monotone in x: got {nodes:?}");
        }
        // The new level (y=5) must cover x=0..2.
        assert_eq!(nodes[0].x, 0);
        assert_eq!(nodes[0].y, 5);
        assert_eq!(nodes[0].width, 2);
    }
}
