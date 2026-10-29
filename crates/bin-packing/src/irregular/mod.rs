//! Native rigid irregular nesting with holes, curved contours and source transforms.

mod engine;
mod geometry;
mod model;

pub use model::*;

use crate::Result;

/// Nest original material shapes under the selected geometry and search controls.
///
/// # Errors
/// Returns structured malformed-geometry, constraint or invalid-result errors.
pub fn solve_irregular(
    problem: IrregularProblem,
    options: IrregularOptions,
) -> Result<IrregularSolution> {
    engine::solve(problem, options)
}

/// Validate every returned source transform and all reported totals independently.
///
/// # Errors
/// Returns malformed-input or invalid-solution errors.
pub fn validate_irregular(problem: &IrregularProblem, solution: &IrregularSolution) -> Result<()> {
    engine::validate(problem, solution)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polygon(vertices: &[(f64, f64)]) -> Shape2D {
        Shape2D {
            components: vec![Polygon2D {
                exterior: Contour2D::Polygon {
                    vertices: vertices.iter().map(|&(x, y)| Point2D { x, y }).collect(),
                },
                holes: Vec::new(),
            }],
        }
    }

    fn rectangle(width: f64, height: f64) -> Shape2D {
        polygon(&[(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)])
    }

    fn bin(shape: Shape2D) -> IrregularBin {
        IrregularBin {
            name: "stock".into(),
            shape,
            cost: 1.0,
            quantity: None,
            defects: Vec::new(),
            spacing: 0.0,
        }
    }

    fn demand(shape: Shape2D, quantity: usize) -> IrregularDemand {
        IrregularDemand {
            name: "part".into(),
            shape,
            quantity,
            allowed_rotations: vec![RotationRange2D { start: 0.0, end: 180.0 }],
            can_mirror: false,
            profit: 1.0,
        }
    }

    fn fixed_demand(shape: Shape2D, quantity: usize) -> IrregularDemand {
        IrregularDemand {
            allowed_rotations: vec![RotationRange2D { start: 0.0, end: 0.0 }],
            ..demand(shape, quantity)
        }
    }

    fn circle(x: f64, y: f64, radius: f64) -> Shape2D {
        Shape2D {
            components: vec![Polygon2D {
                exterior: Contour2D::Circle { center: Point2D { x, y }, radius },
                holes: Vec::new(),
            }],
        }
    }

    fn curve(start: Point2D, segments: Vec<CurveSegment2D>) -> Shape2D {
        Shape2D {
            components: vec![Polygon2D {
                exterior: Contour2D::Curve { start, segments },
                holes: Vec::new(),
            }],
        }
    }

    fn assert_valid(problem: &IrregularProblem, options: IrregularOptions) -> IrregularSolution {
        let result = solve_irregular(problem.clone(), options).expect("irregular packing");
        validate_irregular(problem, &result).expect("independent geometry validation");
        result
    }

    #[test]
    fn geometry_option_limits_reject_nonfinite_and_out_of_range_values() {
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(3.0, 3.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        let reason = "angular step must be 0.1..=180 degrees and curve tolerance 1e-8..=10000";
        for options in [
            IrregularOptions { rotation_step_degrees: f64::NAN, ..IrregularOptions::default() },
            IrregularOptions {
                rotation_step_degrees: f64::INFINITY,
                ..IrregularOptions::default()
            },
            IrregularOptions { rotation_step_degrees: 0.09, ..IrregularOptions::default() },
            IrregularOptions { rotation_step_degrees: 181.0, ..IrregularOptions::default() },
            IrregularOptions { curve_tolerance: f64::INFINITY, ..IrregularOptions::default() },
            IrregularOptions { curve_tolerance: 1e-9, ..IrregularOptions::default() },
            IrregularOptions { curve_tolerance: 10_001.0, ..IrregularOptions::default() },
        ] {
            assert!(matches!(solve_irregular(problem.clone(), options),
                Err(crate::BinPackingError::InvalidConstraint { field, reason: actual })
                    if field == "geometry" && actual == reason));
        }
        for rotation_step_degrees in [0.1, 180.0] {
            let options = IrregularOptions { rotation_step_degrees, ..IrregularOptions::default() };
            assert!(solve_irregular(problem.clone(), options).is_ok());
        }
        for curve_tolerance in [1e-8, 10_000.0] {
            let options = IrregularOptions { curve_tolerance, ..IrregularOptions::default() };
            assert!(solve_irregular(problem.clone(), options).is_ok());
        }
    }

    #[test]
    fn interlocking_l_shapes_use_actual_polygon_material() {
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(2.0, 3.0))],
            demands: vec![demand(
                polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)]),
                2,
            )],
        };
        let result = solve_irregular(problem.clone(), IrregularOptions::default())
            .expect("native irregular packing");
        assert!(result.feasible);
        assert_eq!(
            result.layouts.len(),
            1,
            "bounding rectangles cannot realize this one-bin layout"
        );
        assert_eq!(result.layouts[0].placements.len(), 2);
        validate_irregular(&problem, &result).expect("independent material readback");
    }

    #[test]
    fn unrelated_item_can_fill_another_items_hole() {
        let mut frame = rectangle(4.0, 4.0);
        frame.components[0].holes.push(Contour2D::Polygon {
            vertices: vec![
                Point2D { x: 1.0, y: 1.0 },
                Point2D { x: 1.0, y: 3.0 },
                Point2D { x: 3.0, y: 3.0 },
                Point2D { x: 3.0, y: 1.0 },
            ],
        });
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(4.0, 4.0)) }],
            demands: vec![fixed_demand(frame, 1), fixed_demand(rectangle(2.0, 2.0), 1)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        assert!(result.feasible);
        assert_eq!(result.layouts.len(), 1);
        assert_eq!(result.layouts[0].used_area, 16.0);
        let inner = &result.layouts[0].placements[1];
        assert_eq!((inner.x, inner.y), (1.0, 1.0));
    }

    #[test]
    fn spacing_and_defects_change_material_feasibility() {
        let mut stock = bin(rectangle(5.0, 2.0));
        stock.quantity = Some(1);
        let problem = IrregularProblem {
            bins: vec![stock],
            demands: vec![fixed_demand(rectangle(2.0, 2.0), 2)],
        };
        let spaced = IrregularOptions { item_spacing: 1.0, ..IrregularOptions::default() };
        let result = assert_valid(&problem, spaced.clone());
        assert!(result.feasible);
        let too_far = assert_valid(&problem, IrregularOptions { item_spacing: 1.01, ..spaced });
        assert!(!too_far.feasible);
        assert_eq!(too_far.unplaced_quantities, vec![1]);

        let defect = polygon(&[(2.0, 0.0), (3.0, 0.0), (3.0, 2.0), (2.0, 2.0)]);
        let mut with_defect = problem;
        with_defect.bins[0].defects.push(defect);
        let touching = assert_valid(&with_defect, IrregularOptions::default());
        assert!(touching.feasible, "material can touch a defect boundary at zero clearance");
        let clear = assert_valid(
            &with_defect,
            IrregularOptions { defect_spacing: 0.1, ..IrregularOptions::default() },
        );
        assert_eq!(clear.unplaced_quantities, vec![2]);
    }

    #[test]
    fn polygonal_bin_holes_and_boundary_spacing_are_enforced() {
        let mut stock = rectangle(5.0, 5.0);
        stock.components[0].holes.push(Contour2D::Polygon {
            vertices: vec![
                Point2D { x: 2.0, y: 2.0 },
                Point2D { x: 3.0, y: 2.0 },
                Point2D { x: 3.0, y: 3.0 },
                Point2D { x: 2.0, y: 3.0 },
            ],
        });
        let problem = IrregularProblem {
            bins: vec![bin(stock)],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        let result = assert_valid(
            &problem,
            IrregularOptions { bin_spacing: 0.5, ..IrregularOptions::default() },
        );
        assert!(result.feasible);
        assert!(result.layouts[0].placements[0].x >= 0.5);
        assert!(result.layouts[0].placements[0].y >= 0.5);
        let mut corrupt = result;
        corrupt.layouts[0].placements[0].x = 2.0;
        corrupt.layouts[0].placements[0].y = 2.0;
        assert!(validate_irregular(&problem, &corrupt).is_err());
    }

    #[test]
    fn circles_use_exact_containment_and_tangency_when_both_shapes_are_circles() {
        let same = IrregularProblem {
            bins: vec![bin(circle(5.0, 5.0, 5.0))],
            demands: vec![fixed_demand(circle(0.0, 0.0, 5.0), 1)],
        };
        let result = assert_valid(&same, IrregularOptions::default());
        assert!(result.feasible, "equal exact circles fit without approximation loss");
        assert!((result.layouts[0].used_area - 25.0 * std::f64::consts::PI).abs() < 1e-10);
        let smaller = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(circle(5.0, 5.0, 5.0)) }],
            demands: vec![fixed_demand(circle(0.0, 0.0, 2.0), 2)],
        };
        let result = assert_valid(&smaller, IrregularOptions::default());
        assert!(result.feasible);
        let first = &result.layouts[0].placements[0];
        let second = &result.layouts[0].placements[1];
        assert!((first.x - second.x).hypot(first.y - second.y) >= 4.0);
    }

    #[test]
    fn circular_arcs_keep_analytic_area_and_a_conservative_envelope() {
        let semicircle = Shape2D {
            components: vec![Polygon2D {
                exterior: Contour2D::Curve {
                    start: Point2D { x: 0.0, y: 0.0 },
                    segments: vec![
                        CurveSegment2D::Line { end: Point2D { x: 4.0, y: 0.0 } },
                        CurveSegment2D::Arc {
                            end: Point2D { x: 0.0, y: 0.0 },
                            center: Point2D { x: 2.0, y: 0.0 },
                            clockwise: false,
                        },
                    ],
                },
                holes: Vec::new(),
            }],
        };
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(5.0, 3.0))],
            demands: vec![fixed_demand(semicircle.clone(), 1)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        assert!(result.feasible);
        assert!((result.layouts[0].used_area - 2.0 * std::f64::consts::PI).abs() < 1e-10);
        assert!(result.layouts[0].used_width > 4.0);
        assert!(result.layouts[0].used_height > 2.0);
        let curved_bin = IrregularProblem {
            bins: vec![bin(semicircle.clone())],
            demands: vec![fixed_demand(rectangle(1.0, 0.5), 1)],
        };
        let curved_result = assert_valid(
            &curved_bin,
            IrregularOptions {
                objective: IrregularObjective::OpenDimensionX,
                ..IrregularOptions::default()
            },
        );
        assert!(curved_result.feasible);
        let placement = &curved_result.layouts[0].placements[0];
        assert_eq!(curved_result.layouts[0].used_width, placement.x + 1.0);
        assert_eq!(curved_result.layouts[0].used_height, placement.y + 0.5);
        let exact_box = IrregularProblem {
            bins: vec![bin(rectangle(4.0, 2.0))],
            demands: vec![fixed_demand(semicircle, 1)],
        };
        let result = assert_valid(&exact_box, IrregularOptions::default());
        assert!(!result.feasible, "inscribed arc chords must not permit an unsafe exact fit");
    }

    #[test]
    fn mirroring_and_continuous_interval_samples_use_legal_original_transforms() {
        let stock =
            polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (1.0, 2.0), (1.0, 1.0), (0.0, 1.0)]);
        let part =
            polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)]);
        let mut problem =
            IrregularProblem { bins: vec![bin(stock)], demands: vec![fixed_demand(part, 1)] };
        assert!(!assert_valid(&problem, IrregularOptions::default()).feasible);
        problem.demands[0].can_mirror = true;
        let result = assert_valid(&problem, IrregularOptions::default());
        assert!(result.feasible);
        assert!(result.layouts[0].placements[0].mirrored);
        assert_eq!(result.layouts[0].used_area, 3.0, "reflected L keeps its material area");
        let mut illegal_reflection = result.clone();
        illegal_reflection.layouts[0].placements[0].mirrored = false;
        assert!(
            validate_irregular(&problem, &illegal_reflection).is_err(),
            "unreflected L cannot occupy the reflected stock"
        );
        let mut reflection_not_allowed = problem.clone();
        reflection_not_allowed.demands[0].can_mirror = false;
        assert!(
            matches!(
                validate_irregular(&reflection_not_allowed, &result),
                Err(crate::BinPackingError::InvalidSolution { reason })
                    if reason == "illegal source translation, rotation or reflection"
            ),
            "revoked reflection permission must reject unchanged valid geometry"
        );

        let interval = IrregularProblem {
            bins: vec![bin(rectangle(3.0, 1.0))],
            demands: vec![IrregularDemand {
                allowed_rotations: vec![RotationRange2D { start: 89.0, end: 91.0 }],
                ..fixed_demand(rectangle(1.0, 3.0), 1)
            }],
        };
        let result = assert_valid(
            &interval,
            IrregularOptions { rotation_step_degrees: 20.0, ..IrregularOptions::default() },
        );
        assert!(result.feasible);
        assert_eq!(result.layouts[0].placements[0].rotation_degrees, 90.0);
        assert!(result.search_truncated);
        for (label, angle) in [("below interval", 88.999), ("above interval", 91.001)] {
            let mut illegal = result.clone();
            illegal.layouts[0].placements[0].rotation_degrees = angle;
            assert!(validate_irregular(&interval, &illegal).is_err(), "{label}");
        }
        for (label, start, end) in
            [("below source interval", 90.001, 91.0), ("above source interval", 89.0, 89.999)]
        {
            let mut restricted = interval.clone();
            restricted.demands[0].allowed_rotations = vec![RotationRange2D { start, end }];
            assert!(
                matches!(
                    validate_irregular(&restricted, &result),
                    Err(crate::BinPackingError::InvalidSolution { reason })
                        if reason == "illegal source translation, rotation or reflection"
                ),
                "{label}: unchanged valid geometry must respect source rotation permission"
            );
        }
    }

    #[test]
    fn count_cost_knapsack_and_open_objectives_influence_construction() {
        assert_eq!(
            serde_json::to_string(&IrregularObjective::OpenDimensionXY)
                .expect("objective wire format"),
            "\"open_dimension_xy\""
        );
        let mut cheap = bin(rectangle(4.0, 4.0));
        cheap.cost = 1.0;
        let mut wide = bin(rectangle(8.0, 4.0));
        wide.cost = 10.0;
        let problem = IrregularProblem {
            bins: vec![cheap, wide],
            demands: vec![fixed_demand(rectangle(4.0, 4.0), 2)],
        };
        let count = assert_valid(&problem, IrregularOptions::default());
        assert_eq!(count.layouts.len(), 1);
        assert_eq!(count.total_cost, 10.0);
        let cost = assert_valid(
            &problem,
            IrregularOptions {
                objective: IrregularObjective::TotalCost,
                ..IrregularOptions::default()
            },
        );
        assert_eq!(cost.layouts.len(), 2);
        assert_eq!(cost.total_cost, 2.0);

        let knapsack = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(2.0, 2.0)) }],
            demands: vec![
                IrregularDemand { profit: 10.0, ..fixed_demand(rectangle(2.0, 2.0), 1) },
                IrregularDemand { profit: 2.0, ..fixed_demand(rectangle(1.0, 1.0), 4) },
            ],
        };
        let count = assert_valid(&knapsack, IrregularOptions::default());
        let profit = assert_valid(
            &knapsack,
            IrregularOptions {
                objective: IrregularObjective::Knapsack,
                ..IrregularOptions::default()
            },
        );
        assert!(profit.total_profit > count.total_profit);
        assert_eq!(profit.total_profit, 10.0);
        assert_eq!(profit.unplaced_quantities, vec![0, 4]);

        for (objective, width, height, extent) in [
            (IrregularObjective::OpenDimensionX, 10.0, 2.0, (4.0, 2.0)),
            (IrregularObjective::OpenDimensionY, 2.0, 10.0, (2.0, 4.0)),
            (IrregularObjective::OpenDimensionXY, 10.0, 2.0, (4.0, 2.0)),
        ] {
            let problem = IrregularProblem {
                bins: vec![bin(rectangle(width, height))],
                demands: vec![fixed_demand(rectangle(2.0, 2.0), 2)],
            };
            let result = assert_valid(
                &problem,
                IrregularOptions { objective, ..IrregularOptions::default() },
            );
            assert!(result.feasible);
            assert_eq!(result.layouts.len(), 1);
            assert_eq!((result.layouts[0].used_width, result.layouts[0].used_height), extent);
        }
    }

    #[test]
    fn compound_components_remain_rigid_and_duplicate_labels_keep_provenance() {
        let mut compound = rectangle(1.0, 2.0);
        compound
            .components
            .extend(polygon(&[(2.0, 0.0), (3.0, 0.0), (3.0, 2.0), (2.0, 2.0)]).components);
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(3.0, 2.0)) }],
            demands: vec![fixed_demand(compound, 1), fixed_demand(rectangle(1.0, 2.0), 1)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        assert!(result.feasible);
        assert_eq!(result.unplaced_quantities, vec![0, 0]);
        let inner = result.layouts[0]
            .placements
            .iter()
            .find(|p| p.demand_index == 1)
            .expect("second row provenance");
        assert_eq!((inner.x, inner.y), (1.0, 0.0));
        let encoded = serde_json::to_string(&result).expect("serialize irregular result");
        let decoded: IrregularSolution =
            serde_json::from_str(&encoded).expect("deserialize irregular result");
        validate_irregular(&problem, &decoded).expect("serialized independent readback");
    }

    #[test]
    fn independent_validator_rejects_geometry_transform_inventory_and_total_corruption() {
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(4.0, 2.0)) }],
            demands: vec![fixed_demand(rectangle(2.0, 2.0), 2)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        let mut corruptions = Vec::new();
        let mut corrupt = result.clone();
        corrupt.layouts[0].placements[1].x = corrupt.layouts[0].placements[0].x;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].placements[0].mirrored = true;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].placements[0].rotation_degrees = 360.0;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].placements[0].demand_index = 1;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].placements[0].y = f64::NAN;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].placements[0].y = 1.0;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].used_area += 1.0;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts[0].used_width += 1.0;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.total_cost += 1.0;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.total_profit += 1.0;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.unplaced_quantities[0] = 1;
        corruptions.push(corrupt);
        let mut corrupt = result.clone();
        corrupt.layouts.push(corrupt.layouts[0].clone());
        corruptions.push(corrupt);
        for corrupted in corruptions {
            assert!(matches!(
                validate_irregular(&problem, &corrupted),
                Err(crate::BinPackingError::InvalidSolution { .. })
            ));
        }
    }

    #[test]
    fn malformed_shapes_and_unbounded_controls_fail_at_the_public_boundary() {
        let good = IrregularProblem {
            bins: vec![bin(rectangle(4.0, 4.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        let mut malformed = Vec::new();
        let mut problem = good.clone();
        problem.demands[0].shape = polygon(&[(0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (2.0, 0.0)]);
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].shape.components[0]
            .holes
            .push(Contour2D::Circle { center: Point2D { x: 3.0, y: 3.0 }, radius: 0.2 });
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].shape = circle(0.0, 0.0, f64::INFINITY);
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].profit = f64::NAN;
        malformed.push(problem);
        let mut problem = good.clone();
        problem.bins[0].cost = -1.0;
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].shape = polygon(&[(0.0, 0.0), (f64::NAN, 0.0), (0.0, 1.0)]);
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].shape = rectangle(1_000_001.0, 1.0);
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].allowed_rotations.clear();
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].shape = circle(0.0, 0.0, 1.0);
        problem.demands[0].allowed_rotations.push(RotationRange2D { start: 10.0, end: 9.0 });
        malformed.push(problem);
        let mut problem = good.clone();
        problem.demands[0].quantity = 65;
        malformed.push(problem);
        for problem in malformed {
            assert!(matches!(
                solve_irregular(problem, IrregularOptions::default()),
                Err(crate::BinPackingError::InvalidConstraint { .. })
            ));
        }
        for options in [
            IrregularOptions { max_states: 200_001, ..IrregularOptions::default() },
            IrregularOptions { beam_width: 0, ..IrregularOptions::default() },
            IrregularOptions { item_spacing: -1.0, ..IrregularOptions::default() },
            IrregularOptions { curve_tolerance: f64::NAN, ..IrregularOptions::default() },
        ] {
            assert!(solve_irregular(good.clone(), options).is_err());
        }
    }

    #[test]
    fn exhausted_search_retains_completed_baseline_and_explicit_shortfall() {
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(4.0, 2.0)) }],
            demands: vec![fixed_demand(rectangle(2.0, 2.0), 3)],
        };
        for max_states in [0, 1, 16] {
            let result = assert_valid(
                &problem,
                IrregularOptions { max_states, ..IrregularOptions::default() },
            );
            assert_eq!(result.unplaced_quantities, vec![1]);
            assert_eq!(result.layouts[0].placements.len(), 2);
            assert!(result.explored_states <= max_states);
            assert!(result.construction_probes <= 32_768);
            assert!(result.search_truncated);
            assert_eq!(
                result,
                assert_valid(&problem, result.options.clone()),
                "deterministic bounded search"
            );
        }
        let unavailable = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(0), ..bin(rectangle(4.0, 2.0)) }],
            ..problem
        };
        let result = assert_valid(&unavailable, IrregularOptions::default());
        assert!(result.layouts.is_empty());
        assert_eq!(result.unplaced_quantities, vec![3]);
    }

    fn polygon_ring_contains(contour: &Contour2D, point: Point2D) -> bool {
        let Contour2D::Polygon { vertices } = contour else {
            return false;
        };
        let mut inside = false;
        for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
            if (first.y > point.y) != (second.y > point.y)
                && point.x
                    < first.x + (point.y - first.y) * (second.x - first.x) / (second.y - first.y)
            {
                inside = !inside;
            }
        }
        inside
    }

    // A separate ray-crossing occupancy oracle deliberately does not call geo
    // or the production reconstruction/containment helpers.
    fn polygon_material_at(shape: &Shape2D, point: Point2D) -> bool {
        shape.components.iter().any(|component| {
            polygon_ring_contains(&component.exterior, point)
                && !component.holes.iter().any(|hole| polygon_ring_contains(hole, point))
        })
    }

    #[test]
    fn integer_contact_layouts_match_an_independent_cell_occupancy_oracle() {
        let mut holed = rectangle(3.0, 3.0);
        holed.components[0].holes.push(Contour2D::Polygon {
            vertices: vec![
                Point2D { x: 1.0, y: 1.0 },
                Point2D { x: 2.0, y: 1.0 },
                Point2D { x: 2.0, y: 2.0 },
                Point2D { x: 1.0, y: 2.0 },
            ],
        });
        let fixtures = [
            IrregularProblem {
                bins: vec![bin(rectangle(2.0, 3.0))],
                demands: vec![IrregularDemand {
                    allowed_rotations: [0.0, 90.0, 180.0, 270.0]
                        .into_iter()
                        .map(|angle| RotationRange2D { start: angle, end: angle })
                        .collect(),
                    ..fixed_demand(
                        polygon(&[
                            (0.0, 0.0),
                            (2.0, 0.0),
                            (2.0, 1.0),
                            (1.0, 1.0),
                            (1.0, 2.0),
                            (0.0, 2.0),
                        ]),
                        2,
                    )
                }],
            },
            IrregularProblem {
                bins: vec![bin(rectangle(3.0, 3.0))],
                demands: vec![fixed_demand(holed, 1), fixed_demand(rectangle(1.0, 1.0), 1)],
            },
        ];
        for problem in fixtures {
            let result = assert_valid(&problem, IrregularOptions::default());
            assert!(result.feasible);
            assert_eq!(result.layouts.len(), 1);
            let mut occupied = 0usize;
            for x in 0..12 {
                for y in 0..12 {
                    let world =
                        Point2D { x: f64::from(x) / 4.0 + 0.125, y: f64::from(y) / 4.0 + 0.125 };
                    let mut material_count = 0;
                    for placement in &result.layouts[0].placements {
                        assert_eq!(placement.x.fract(), 0.0);
                        assert_eq!(placement.y.fract(), 0.0);
                        let dx = world.x - placement.x;
                        let dy = world.y - placement.y;
                        let (x, y) = match placement.rotation_degrees as u32 {
                            0 => (dx, dy),
                            90 => (dy, -dx),
                            180 => (-dx, -dy),
                            270 => (-dy, dx),
                            _ => {
                                assert_eq!(placement.rotation_degrees, 0.0);
                                (dx, dy)
                            }
                        };
                        let local = Point2D { x: if placement.mirrored { -x } else { x }, y };
                        material_count += usize::from(polygon_material_at(
                            &problem.demands[placement.demand_index].shape,
                            local,
                        ));
                    }
                    assert!(material_count <= 1, "independent material occupancy overlap");
                    if material_count == 1 {
                        assert!(polygon_material_at(&problem.bins[0].shape, world));
                        occupied += 1;
                    }
                }
            }
            assert_eq!(occupied as f64 / 16.0, result.layouts[0].used_area);
        }
    }

    #[test]
    fn analytic_arc_validation_rejects_crossings_overlap_radius_and_tolerance_errors() {
        let mut problem = IrregularProblem {
            bins: vec![bin(rectangle(10.0, 10.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        for segments in [
            vec![
                CurveSegment2D::Arc {
                    end: Point2D { x: 0.0, y: 1.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: false,
                },
                CurveSegment2D::Line { end: Point2D { x: 1.0, y: 0.0 } },
            ],
            vec![
                CurveSegment2D::Arc {
                    end: Point2D { x: -1.0, y: 0.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: false,
                },
                CurveSegment2D::Arc {
                    end: Point2D { x: 0.0, y: 1.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: true,
                },
                CurveSegment2D::Line { end: Point2D { x: 1.0, y: 0.0 } },
            ],
            vec![
                CurveSegment2D::Arc {
                    end: Point2D { x: -1.0, y: 0.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: false,
                },
                CurveSegment2D::Line { end: Point2D { x: 0.0, y: 2.0 } },
                CurveSegment2D::Line { end: Point2D { x: 1.0, y: 0.0 } },
            ],
        ]
        .into_iter()
        .enumerate()
        {
            let (index, mut segments) = segments;
            if index == 0
                && let CurveSegment2D::Arc { end, .. } = &mut segments[0]
            {
                end.y = 2.0;
            }
            problem.demands[0].shape.components[0].exterior =
                Contour2D::Curve { start: Point2D { x: 1.0, y: 0.0 }, segments };
            assert!(matches!(
                solve_irregular(problem.clone(), IrregularOptions::default()),
                Err(crate::BinPackingError::InvalidConstraint { .. })
            ));
        }
        problem.demands[0].shape = circle(0.0, 0.0, 10.0);
        assert!(
            solve_irregular(
                problem,
                IrregularOptions { curve_tolerance: 1e-8, ..IrregularOptions::default() }
            )
            .is_err()
        );
    }

    #[test]
    fn rounded_collapsed_transforms_return_a_valid_partial_result() {
        let problem = IrregularProblem {
            bins: vec![bin(polygon(&[
                (0.0, 1_000_000.0),
                (2.0, 1_000_000.0),
                (2.0, 999_999.0),
                (0.0, 999_999.0),
            ]))],
            demands: vec![fixed_demand(rectangle(1.0, 1e-12), 1)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        assert_eq!(result.unplaced_quantities, vec![1]);
    }

    #[test]
    fn contact_vertex_sampling_always_marks_search_truncated() {
        for vertices in 64..=68 {
            let points: Vec<_> = (0..vertices)
                .map(|index| {
                    let (sin, cos) =
                        (std::f64::consts::TAU * f64::from(index) / f64::from(vertices)).sin_cos();
                    (0.9 * cos, 0.9 * sin)
                })
                .collect();
            let problem = IrregularProblem {
                bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(2.0, 2.0)) }],
                demands: vec![fixed_demand(polygon(&points), 1)],
            };
            let result = assert_valid(
                &problem,
                IrregularOptions {
                    max_states: 200_000,
                    beam_width: 32,
                    max_candidates_per_item: 512,
                    ..IrregularOptions::default()
                },
            );
            assert!(result.feasible);
            assert_eq!(
                result.search_truncated,
                vertices > 64,
                "sampling status must match the actual unique vertex count {vertices}"
            );
        }
    }

    #[test]
    fn irregular_wire_inputs_reject_unknown_physical_fields() {
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(2.0, 2.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        let original = serde_json::to_value(problem).expect("problem wire format");
        let mut invalid = Vec::new();
        let mut value = original.clone();
        value["demands"][0]["item_clearance"] = serde_json::json!(1);
        invalid.push(value);
        let mut value = original.clone();
        value["bins"][0]["shape"]["components"][0]["exterior"]["ignored_holes"] =
            serde_json::json!([]);
        invalid.push(value);
        let mut value = original;
        value["demands"][0]["allowed_rotations"][0]["mirror"] = serde_json::json!(false);
        invalid.push(value);
        for value in invalid {
            assert!(serde_json::from_value::<IrregularProblem>(value).is_err());
        }
        let mut options =
            serde_json::to_value(IrregularOptions::default()).expect("options wire format");
        options["defect_clearance"] = serde_json::json!(1);
        assert!(serde_json::from_value::<IrregularOptions>(options).is_err());
    }

    #[test]
    fn two_arc_lens_area_matches_the_circle_intersection_formula_for_both_windings() {
        let height = 3.0_f64.sqrt() / 2.0;
        let lower = Point2D { x: 0.5, y: -height };
        let upper = Point2D { x: 0.5, y: height };
        for clockwise in [false, true] {
            let (start, end) = if clockwise { (upper, lower) } else { (lower, upper) };
            let lens = curve(
                start,
                vec![
                    CurveSegment2D::Arc { end, center: Point2D { x: 0.0, y: 0.0 }, clockwise },
                    CurveSegment2D::Arc {
                        end: start,
                        center: Point2D { x: 1.0, y: 0.0 },
                        clockwise,
                    },
                ],
            );
            let problem = IrregularProblem {
                bins: vec![bin(rectangle(3.0, 3.0))],
                demands: vec![fixed_demand(lens, 1)],
            };
            let result = assert_valid(&problem, IrregularOptions::default());
            assert!(result.feasible);
            // Intersection area of two unit disks whose centers are one unit apart.
            let analytic_area = 2.0 * std::f64::consts::PI / 3.0 - height;
            assert!((result.layouts[0].used_area - analytic_area).abs() < 1e-12);
        }
    }

    #[test]
    fn full_circle_and_two_semicircle_curves_preserve_area_in_either_direction() {
        let start = Point2D { x: 1.0, y: 0.0 };
        for clockwise in [false, true] {
            for split in [false, true] {
                let endpoints =
                    if split { vec![Point2D { x: -1.0, y: 0.0 }, start] } else { vec![start] };
                let round = curve(
                    start,
                    endpoints
                        .into_iter()
                        .map(|end| CurveSegment2D::Arc {
                            end,
                            center: Point2D { x: 0.0, y: 0.0 },
                            clockwise,
                        })
                        .collect(),
                );
                let problem = IrregularProblem {
                    bins: vec![bin(rectangle(3.0, 3.0))],
                    demands: vec![fixed_demand(round, 1)],
                };
                let result = assert_valid(&problem, IrregularOptions::default());
                assert!(result.feasible);
                assert!((result.layouts[0].used_area - std::f64::consts::PI).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn disjoint_and_intersecting_support_circles_do_not_make_capsule_arcs_cross() {
        for half_length in [0.25, 0.5, 1.0, 1.5] {
            let capsule = curve(
                Point2D { x: -half_length, y: -1.0 },
                vec![
                    CurveSegment2D::Line { end: Point2D { x: half_length, y: -1.0 } },
                    CurveSegment2D::Arc {
                        end: Point2D { x: half_length, y: 1.0 },
                        center: Point2D { x: half_length, y: 0.0 },
                        clockwise: false,
                    },
                    CurveSegment2D::Line { end: Point2D { x: -half_length, y: 1.0 } },
                    CurveSegment2D::Arc {
                        end: Point2D { x: -half_length, y: -1.0 },
                        center: Point2D { x: -half_length, y: 0.0 },
                        clockwise: false,
                    },
                ],
            );
            let problem = IrregularProblem {
                bins: vec![bin(rectangle(6.0, 3.0))],
                demands: vec![fixed_demand(capsule, 1)],
            };
            let result = assert_valid(&problem, IrregularOptions::default());
            assert!(result.feasible);
            let rectangle_and_two_half_disks = 4.0 * half_length + std::f64::consts::PI;
            assert!((result.layouts[0].used_area - rectangle_and_two_half_disks).abs() < 1e-12);
        }
    }

    #[test]
    fn annular_sector_with_concentric_arcs_has_the_independent_sector_area() {
        let sector = curve(
            Point2D { x: 2.0, y: 0.0 },
            vec![
                CurveSegment2D::Arc {
                    end: Point2D { x: 0.0, y: 2.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: false,
                },
                CurveSegment2D::Line { end: Point2D { x: 0.0, y: 1.0 } },
                CurveSegment2D::Arc {
                    end: Point2D { x: 1.0, y: 0.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: true,
                },
                CurveSegment2D::Line { end: Point2D { x: 2.0, y: 0.0 } },
            ],
        );
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(3.0, 3.0))],
            demands: vec![fixed_demand(sector, 1)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        assert!(result.feasible);
        assert!((result.layouts[0].used_area - 3.0 * std::f64::consts::PI / 4.0).abs() < 1e-12);
    }

    #[test]
    fn exact_circle_validator_matches_independent_clearance_inequalities() {
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(circle(0.0, 0.0, 5.0)) }],
            demands: vec![fixed_demand(circle(0.0, 0.0, 1.0), 2)],
        };
        let baseline = assert_valid(&problem, IrregularOptions::default());
        assert!(baseline.feasible);
        assert_eq!(baseline.layouts.len(), 1);
        for (item_spacing, bin_spacing) in [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5), (0.5, 0.5)] {
            for x in -4..=4 {
                for y in -4..=4 {
                    let x = f64::from(x);
                    let y = f64::from(y);
                    let mut candidate = baseline.clone();
                    candidate.options.item_spacing = item_spacing;
                    candidate.options.bin_spacing = bin_spacing;
                    candidate.layouts[0].placements[0].x = -2.0;
                    candidate.layouts[0].placements[0].y = 0.0;
                    candidate.layouts[0].placements[1].x = x;
                    candidate.layouts[0].placements[1].y = y;
                    candidate.layouts[0].used_width = (-2.0_f64).max(x) + 6.0;
                    candidate.layouts[0].used_height = 0.0_f64.max(y) + 6.0;
                    let fits_bin = x.hypot(y) + 1.0 + bin_spacing <= 5.0;
                    let clears_first_item = (x + 2.0).hypot(y) >= 2.0 + item_spacing;
                    assert_eq!(
                        validate_irregular(&problem, &candidate).is_ok(),
                        fits_bin && clears_first_item,
                        "circle ({x},{y}), item clearance {item_spacing}, bin clearance {bin_spacing}"
                    );
                }
            }
        }
    }

    #[test]
    fn circular_holes_and_rigid_components_reject_uncertain_separation() {
        for gap in [0.03125, 0.125] {
            let mut annulus = circle(0.0, 0.0, 3.0);
            annulus.components[0]
                .holes
                .push(Contour2D::Circle { center: Point2D { x: 0.0, y: 0.0 }, radius: 3.0 - gap });
            let mut compound = circle(0.0, 0.0, 0.5);
            compound.components.extend(circle(1.0 + gap, 0.0, 0.5).components);
            for shape in [annulus, compound] {
                let problem = IrregularProblem {
                    bins: vec![bin(rectangle(8.0, 8.0))],
                    demands: vec![fixed_demand(shape, 1)],
                };
                let result = solve_irregular(problem.clone(), IrregularOptions::default());
                if gap < 0.04 {
                    assert!(matches!(
                        result,
                        Err(crate::BinPackingError::InvalidConstraint { field, .. })
                            if field == "curve_tolerance"
                    ));
                } else {
                    let result = result.expect("resolved curved topology");
                    assert!(result.feasible);
                    validate_irregular(&problem, &result).expect("curved topology readback");
                }
            }
        }
    }

    #[test]
    fn circular_bin_hole_readback_satisfies_exact_radial_bounds() {
        let mut annulus = circle(0.0, 0.0, 3.0);
        annulus.components[0]
            .holes
            .push(Contour2D::Circle { center: Point2D { x: 0.0, y: 0.0 }, radius: 1.0 });
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(annulus) }],
            demands: vec![fixed_demand(circle(0.0, 0.0, 0.5), 2)],
        };
        let result = assert_valid(&problem, IrregularOptions::default());
        assert!(result.feasible);
        for placement in &result.layouts[0].placements {
            let radial_distance = placement.x.hypot(placement.y);
            assert!(radial_distance - 0.5 >= 1.0, "source circle must clear the original hole");
            assert!(radial_distance + 0.5 <= 3.0, "source circle must remain in the original bin");
        }
    }

    #[test]
    fn malformed_contours_and_nonfinite_or_overflowing_values_have_structured_errors() {
        let good = IrregularProblem {
            bins: vec![bin(rectangle(8.0, 8.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        for shape in [
            Shape2D { components: Vec::new() },
            polygon(&[(0.0, 0.0), (1.0, 0.0)]),
            polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 0.0), (0.0, 1.0)]),
            polygon(&[(0.0, 0.0), (1.0, 0.0), (0.0, 0.0)]),
            curve(Point2D { x: 0.0, y: 0.0 }, Vec::new()),
            curve(
                Point2D { x: 0.0, y: 0.0 },
                vec![CurveSegment2D::Line { end: Point2D { x: 0.0, y: 0.0 } }],
            ),
            curve(
                Point2D { x: 0.0, y: 0.0 },
                vec![CurveSegment2D::Line { end: Point2D { x: 1.0, y: 0.0 } }],
            ),
            curve(
                Point2D { x: 0.0, y: 0.0 },
                vec![CurveSegment2D::Arc {
                    end: Point2D { x: 0.0, y: 0.0 },
                    center: Point2D { x: 0.0, y: 0.0 },
                    clockwise: false,
                }],
            ),
            circle(1_000_000.0, 0.0, 1.0),
        ] {
            let mut problem = good.clone();
            problem.demands[0].shape = shape;
            assert!(matches!(
                solve_irregular(problem, IrregularOptions::default()),
                Err(crate::BinPackingError::InvalidConstraint { .. })
            ));
        }
        for value in [f64::INFINITY, f64::MAX] {
            let mut problem = good.clone();
            problem.bins[0].cost = value;
            assert!(matches!(
                solve_irregular(problem, IrregularOptions::default()),
                Err(crate::BinPackingError::InvalidConstraint { .. })
            ));
            let mut problem = good.clone();
            problem.demands[0].profit = value;
            assert!(matches!(
                solve_irregular(problem, IrregularOptions::default()),
                Err(crate::BinPackingError::InvalidConstraint { .. })
            ));
        }
        for options in [
            IrregularOptions { max_candidates_per_item: 0, ..IrregularOptions::default() },
            IrregularOptions { max_orientations: 0, ..IrregularOptions::default() },
            IrregularOptions { rotation_step_degrees: 0.0, ..IrregularOptions::default() },
            IrregularOptions { bin_spacing: f64::INFINITY, ..IrregularOptions::default() },
            IrregularOptions { defect_spacing: f64::NAN, ..IrregularOptions::default() },
        ] {
            assert!(matches!(
                solve_irregular(good.clone(), options),
                Err(crate::BinPackingError::InvalidConstraint { .. })
            ));
        }
    }

    #[test]
    fn independent_readback_rejects_forged_budgets_inventory_and_feasibility_metadata() {
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(2), ..bin(rectangle(1.0, 1.0)) }],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 2)],
        };
        let baseline = assert_valid(&problem, IrregularOptions::default());
        assert!(baseline.feasible);
        assert_eq!(baseline.layouts.len(), 2);
        let mutations: [fn(&mut IrregularSolution); 9] = [
            |result| result.algorithm.clear(),
            |result| result.unplaced_quantities.clear(),
            |result| result.explored_states = result.options.max_states.saturating_add(1),
            |result| result.construction_probes = 32_769,
            |result| result.layouts[0].bin_index = usize::MAX,
            |result| result.layouts[0].placements.clear(),
            |result| result.feasible = false,
            |result| result.layouts[0].used_height = f64::NAN,
            |result| result.options.objective = IrregularObjective::OpenDimensionX,
        ];
        for (index, mutate) in mutations.into_iter().enumerate() {
            let mut corrupt = baseline.clone();
            mutate(&mut corrupt);
            assert!(
                matches!(
                    validate_irregular(&problem, &corrupt),
                    Err(crate::BinPackingError::InvalidSolution { .. })
                ),
                "forged result {index} must be rejected"
            );
        }

        let separate_sources = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(2), ..bin(rectangle(1.0, 1.0)) }],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1); 2],
        };
        let valid = assert_valid(&separate_sources, IrregularOptions::default());
        assert_eq!(valid.layouts.len(), 2, "one unit of stock per source copy");
        assert_eq!(valid.unplaced_quantities, vec![0, 0], "both source copies accounted for");
        let mut duplicate = valid.clone();
        duplicate.layouts[1].placements[0].demand_index =
            duplicate.layouts[0].placements[0].demand_index;
        assert!(
            matches!(
                validate_irregular(&separate_sources, &duplicate),
                Err(crate::BinPackingError::InvalidSolution { reason })
                    if reason == "source demand was placed too many times"
            ),
            "a second identical shape cannot consume the first source twice"
        );

        let contact = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), ..bin(rectangle(2.0, 1.0)) }],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 2)],
        };
        let touching = assert_valid(&contact, IrregularOptions::default());
        let placements = &touching.layouts[0].placements;
        assert_eq!(placements.len(), 2, "both unit squares fit at contact");
        assert_eq!((placements[0].x - placements[1].x).abs(), 1.0, "unit squares touch");
        assert_eq!(touching.layouts[0].used_area, 2.0, "two unit squares cover the stock");
        let mut overlapping = touching.clone();
        overlapping.layouts[0].placements[1].x = overlapping.layouts[0].placements[0].x;
        assert!(
            validate_irregular(&contact, &overlapping).is_err(),
            "coincident unit squares overlap even though each is inside the stock"
        );
    }

    #[test]
    fn omitted_wire_controls_use_documented_cost_profit_inventory_and_rotation_defaults() {
        let encoded = serde_json::json!({
            "bins": [{ "name": "stock", "shape": rectangle(1.0, 1.0) }],
            "demands": [{ "name": "part", "shape": rectangle(1.0, 1.0), "quantity": 2 }]
        });
        let problem: IrregularProblem =
            serde_json::from_value(encoded).expect("minimal wire input");
        let options: IrregularOptions = serde_json::from_str("{}").expect("default wire options");
        assert_eq!(options, IrregularOptions::default());
        assert_eq!(problem.bins[0].quantity, None);
        assert_eq!(
            problem.demands[0].allowed_rotations,
            vec![RotationRange2D { start: 0.0, end: 0.0 }]
        );
        assert!(!problem.demands[0].can_mirror);
        let result = assert_valid(&problem, options);
        assert!(result.feasible);
        assert_eq!(result.layouts.len(), 2);
        assert_eq!(result.total_cost, 2.0);
        assert_eq!(result.total_profit, 2.0);
    }

    #[test]
    fn terra_batch2_total_quantity_and_open_objective_limits() {
        let mut problem = IrregularProblem {
            bins: vec![bin(rectangle(4.0, 4.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 33); 2],
        };
        assert!(matches!(
            solve_irregular(problem.clone(), IrregularOptions::default()),
            Err(crate::BinPackingError::InvalidConstraint { field, .. })
                if field == "demands.quantity"
        ));
        problem.demands[1].quantity = 1;
        problem.bins.push(bin(rectangle(4.0, 4.0)));
        for objective in [
            IrregularObjective::OpenDimensionX,
            IrregularObjective::OpenDimensionY,
            IrregularObjective::OpenDimensionXY,
        ] {
            assert!(matches!(
                solve_irregular(problem.clone(), IrregularOptions { objective, ..IrregularOptions::default() }),
                Err(crate::BinPackingError::InvalidConstraint { field, .. }) if field == "objective"
            ));
        }
    }

    #[test]
    fn terra_batch2_defect_outside_stock_is_rejected() {
        let mut stock = bin(rectangle(4.0, 4.0));
        stock.defects.push(polygon(&[(3.0, 3.0), (5.0, 3.0), (5.0, 4.0), (3.0, 4.0)]));
        let problem = IrregularProblem {
            bins: vec![stock],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        assert!(matches!(
            solve_irregular(problem, IrregularOptions::default()),
            Err(crate::BinPackingError::InvalidConstraint { field, .. }) if field == "bins.defects"
        ));
    }

    #[test]
    fn terra_batch2_orientation_cap_counts_reflections() {
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(4.0, 4.0))],
            demands: vec![IrregularDemand {
                can_mirror: true,
                allowed_rotations: vec![
                    RotationRange2D { start: 0.0, end: 0.0 },
                    RotationRange2D { start: 90.0, end: 90.0 },
                ],
                ..fixed_demand(rectangle(1.0, 2.0), 1)
            }],
        };
        let options = IrregularOptions { max_orientations: 3, ..IrregularOptions::default() };
        assert!(matches!(
            solve_irregular(problem.clone(), options),
            Err(crate::BinPackingError::InvalidConstraint { field, .. }) if field == "max_orientations"
        ));
        let result = assert_valid(
            &problem,
            IrregularOptions { max_orientations: 4, ..IrregularOptions::default() },
        );
        assert!(result.feasible);
        assert_eq!(result.layouts[0].placements.len(), 1);
    }

    #[test]
    fn terra_batch2_partial_inventory_preserves_source_counts_and_options() {
        let problem = IrregularProblem {
            bins: vec![IrregularBin { quantity: Some(1), cost: 3.0, ..bin(rectangle(1.0, 1.0)) }],
            demands: vec![
                IrregularDemand { profit: 2.0, ..fixed_demand(rectangle(1.0, 1.0), 1) },
                IrregularDemand { profit: 7.0, ..fixed_demand(rectangle(1.0, 1.0), 1) },
            ],
        };
        let options = IrregularOptions {
            objective: IrregularObjective::Knapsack,
            max_states: 0,
            ..IrregularOptions::default()
        };
        let result = assert_valid(&problem, options.clone());
        assert_eq!(result.options, options);
        assert!(!result.feasible);
        assert_eq!(result.layouts.len(), 1);
        let index = result.layouts[0].placements[0].demand_index;
        assert!(index < 2);
        assert_eq!(result.unplaced_quantities[index], 0);
        assert_eq!(result.unplaced_quantities[1 - index], 1);
        assert_eq!(result.total_cost, 3.0);
        assert_eq!(result.total_profit, problem.demands[index].profit);
        assert_eq!(result.explored_states, 0);
    }

    #[test]
    fn input_boundary_matrix_reports_the_specific_constraint() {
        let good = IrregularProblem {
            bins: vec![bin(rectangle(4.0, 4.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        let valid = assert_valid(&good, IrregularOptions::default());
        assert!(valid.feasible);
        let mut cases: Vec<(IrregularProblem, &str, &str)> = Vec::new();
        let problem_reason = "requires 1..=8 stocks, 1..=32 demands and 1..=64 copies per row";
        for mutate in [
            (|p: &mut IrregularProblem| p.bins.clear()) as fn(&mut IrregularProblem),
            |p| p.bins = vec![p.bins[0].clone(); 9],
            |p| p.demands.clear(),
            |p| p.demands = vec![p.demands[0].clone(); 33],
            |p| p.demands[0].quantity = 0,
            |p| p.demands[0].quantity = 65,
        ] {
            let mut problem = good.clone();
            mutate(&mut problem);
            cases.push((problem, "problem", problem_reason));
        }
        for mutate in [
            (|p: &mut IrregularProblem| p.bins[0].name = "  ".into()) as fn(&mut IrregularProblem),
            |p| p.bins[0].cost = -1.0,
            |p| p.bins[0].spacing = f64::NAN,
            |p| p.bins[0].spacing = 1_000_001.0,
            |p| p.bins[0].defects = vec![rectangle(1.0, 1.0); 33],
        ] {
            let mut problem = good.clone();
            mutate(&mut problem);
            cases.push((
                problem,
                "bins",
                "invalid label, cost, spacing, defect count or aggregate cost",
            ));
        }
        for mutate in [
            (|p: &mut IrregularProblem| p.demands[0].name.clear()) as fn(&mut IrregularProblem),
            |p| p.demands[0].profit = -1.0,
            |p| p.demands[0].allowed_rotations.clear(),
            |p| {
                p.demands[0].allowed_rotations = vec![RotationRange2D { start: 0.0, end: 0.0 }; 129]
            },
        ] {
            let mut problem = good.clone();
            mutate(&mut problem);
            cases.push((problem, "demands", "invalid label, profit or rotation range count"));
        }
        for range in [
            RotationRange2D { start: f64::NAN, end: 0.0 },
            RotationRange2D { start: 0.0, end: f64::INFINITY },
            RotationRange2D { start: -0.1, end: 0.0 },
            RotationRange2D { start: 361.0, end: 361.0 },
            RotationRange2D { start: 90.0, end: 89.0 },
            RotationRange2D { start: 0.0, end: 361.0 },
        ] {
            let mut problem = good.clone();
            problem.demands[0].allowed_rotations = vec![range];
            cases.push((
                problem,
                "allowed_rotations",
                "requires inclusive finite intervals in 0..=360 degrees",
            ));
        }
        for (problem, expected_field, expected_reason) in cases {
            assert!(
                matches!(
                    solve_irregular(problem, IrregularOptions::default()),
                    Err(crate::BinPackingError::InvalidConstraint { field, reason })
                        if field == expected_field && reason == expected_reason
                ),
                "expected {expected_field}: {expected_reason}"
            );
        }
        for range in
            [RotationRange2D { start: 0.0, end: 0.0 }, RotationRange2D { start: 360.0, end: 360.0 }]
        {
            let mut problem = good.clone();
            problem.bins[0].cost = 0.0;
            problem.demands[0].profit = 0.0;
            problem.demands[0].allowed_rotations = vec![range];
            assert!(assert_valid(&problem, IrregularOptions::default()).feasible);
        }
    }

    #[test]
    fn readback_rejects_each_illegal_placement_coordinate() {
        let problem = IrregularProblem {
            bins: vec![bin(rectangle(4.0, 4.0))],
            demands: vec![fixed_demand(rectangle(1.0, 1.0), 1)],
        };
        let baseline = assert_valid(&problem, IrregularOptions::default());
        let mutations: [fn(&mut IrregularPlacement); 7] = [
            |p| p.x = f64::NAN,
            |p| p.y = f64::INFINITY,
            |p| p.x = 4_000_001.0,
            |p| p.y = -4_000_001.0,
            |p| p.rotation_degrees = f64::NAN,
            |p| p.rotation_degrees = 1.0,
            |p| p.mirrored = true,
        ];
        for mutate in mutations {
            let mut result = baseline.clone();
            mutate(&mut result.layouts[0].placements[0]);
            assert!(matches!(validate_irregular(&problem, &result),
                Err(crate::BinPackingError::InvalidSolution { reason })
                    if reason == "illegal source translation, rotation or reflection"));
        }
    }

    #[test]
    fn periodic_discrete_rotations_keep_a_legal_witness_with_one_orientation_slot() {
        for angles in [vec![360.0], vec![0.0, 360.0]] {
            let expected = angles[0];
            let problem = IrregularProblem {
                bins: vec![bin(rectangle(1.0, 1.0))],
                demands: vec![IrregularDemand {
                    allowed_rotations: angles
                        .into_iter()
                        .map(|angle| RotationRange2D { start: angle, end: angle })
                        .collect(),
                    ..fixed_demand(rectangle(1.0, 1.0), 1)
                }],
            };
            let result = assert_valid(
                &problem,
                IrregularOptions { max_orientations: 1, ..IrregularOptions::default() },
            );
            assert!(result.feasible);
            assert_eq!(result.layouts[0].placements[0].rotation_degrees, expected);
        }
    }
}
