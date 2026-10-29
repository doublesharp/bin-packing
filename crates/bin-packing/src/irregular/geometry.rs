//! Validated source geometry, conservative circular envelopes and rigid transforms.

use std::f64::consts::TAU;

use geo::algorithm::buffer::{BufferStyle, LineJoin};
use geo::coordinate_position::CoordPos;
use geo::dimensions::Dimensions;
use geo::line_intersection::{LineIntersection, line_intersection};
use geo::{
    Area, BoundingRect, Buffer, Coord, CoordsIter, Distance, Euclidean, Line, LineString,
    MapCoords, MultiLineString, MultiPolygon, Polygon, Rect, Relate, Validation,
};

use super::{Contour2D, CurveSegment2D, IrregularOptions, Point2D, Shape2D};
use crate::{BinPackingError, Result};

#[cfg(test)]
mod intersection_tests {
    use super::*;

    fn point(x: f64, y: f64) -> Point2D {
        Point2D { x, y }
    }

    fn assert_constraint<T>(result: Result<T>, expected_field: &str, expected_reason: &str) {
        assert!(matches!(result, Err(BinPackingError::InvalidConstraint { field, reason })
            if field == expected_field && reason == expected_reason));
    }

    #[test]
    fn coordinate_and_circle_bounds_are_checked_independently() {
        for p in [
            point(f64::NAN, 0.0),
            point(0.0, f64::INFINITY),
            point(1_000_001.0, 0.0),
            point(0.0, -1_000_001.0),
        ] {
            assert_constraint(
                checked_point(p),
                "shape",
                "coordinates must be finite and within +/-1000000",
            );
        }
        assert!(checked_point(point(-MAX_COORDINATE, MAX_COORDINATE)).is_ok());
        for radius in [f64::NAN, f64::INFINITY, 0.0, -1.0, 1_000_001.0] {
            assert_constraint(
                flatten(&Contour2D::Circle { center: point(0.0, 0.0), radius }, 0.01, &mut 0),
                "circle.radius",
                "must be finite and in (0,1000000]",
            );
        }
        assert_constraint(
            flatten(
                &Contour2D::Circle { center: point(MAX_COORDINATE, 0.0), radius: 1.0 },
                0.01,
                &mut 0,
            ),
            "shape",
            "coordinates must be finite and within +/-1000000",
        );
        assert!(
            flatten(&Contour2D::Circle { center: point(0.0, 0.0), radius: 1.0 }, 0.01, &mut 0)
                .is_ok()
        );
        assert_constraint(
            arc(
                point(MAX_COORDINATE, MAX_COORDINATE),
                point(MAX_COORDINATE, MAX_COORDINATE),
                point(0.0, 0.0),
                false,
            ),
            "arc",
            "endpoints must have the same positive finite radius",
        );
    }

    #[test]
    fn contour_and_shape_count_limits_report_their_own_constraints() {
        for vertices in [vec![], vec![point(0.0, 0.0); MAX_RING_POINTS + 1]] {
            assert_constraint(
                flatten(&Contour2D::Polygon { vertices }, 0.01, &mut 0),
                "shape",
                "polygon contours need 3..=2048 vertices",
            );
        }
        let segment = CurveSegment2D::Line { end: point(1.0, 0.0) };
        for segments in [vec![], vec![segment; MAX_RING_POINTS + 1]] {
            assert_constraint(
                flatten(&Contour2D::Curve { start: point(0.0, 0.0), segments }, 0.01, &mut 0),
                "curve",
                "needs 1..=2048 segments",
            );
        }
        let contour = Contour2D::Polygon {
            vertices: vec![point(0.0, 0.0), point(1.0, 0.0), point(0.0, 1.0)],
        };
        assert!(flatten(&contour, 0.01, &mut 0).is_ok());
        let mut total = MAX_SOURCE_POINTS;
        assert_constraint(
            flatten(&contour, 0.01, &mut total),
            "shape",
            "prepared geometry exceeds 8192 source vertices",
        );
        let component = super::super::Polygon2D { exterior: contour.clone(), holes: Vec::new() };
        let options = IrregularOptions::default();
        for components in [vec![], vec![component.clone(); 17]] {
            assert_constraint(
                prepare_shape(&Shape2D { components }, &options, Role::Item),
                "shape.components",
                "must contain 1..=16 rigid components",
            );
        }
        let mut excessive_holes = component.clone();
        excessive_holes.holes = vec![contour; 33];
        assert_constraint(
            prepare_shape(&Shape2D { components: vec![excessive_holes] }, &options, Role::Item),
            "shape.holes",
            "at most 32 holes per component are supported",
        );
        assert!(
            prepare_shape(&Shape2D { components: vec![component] }, &options, Role::Item).is_ok()
        );
    }

    #[test]
    fn curved_resolution_rejects_tolerance_below_coordinate_scale() {
        let shape = Shape2D {
            components: vec![super::super::Polygon2D {
                exterior: Contour2D::Circle { center: point(900_000.0, 0.0), radius: 1.0 },
                holes: Vec::new(),
            }],
        };
        let options = IrregularOptions { curve_tolerance: 0.0001, ..IrregularOptions::default() };
        assert_constraint(
            prepare_shape(&shape, &options, Role::Item),
            "curve_tolerance",
            "too small for the coordinate scale; rescale or increase tolerance",
        );
        assert!(prepare_shape(&shape, &IrregularOptions::default(), Role::Item).is_ok());
    }

    #[test]
    fn circular_and_linear_intersections_handle_separation_and_tangency() {
        let full = |x, radius| {
            arc(point(x + radius, 0.0), point(x + radius, 0.0), point(x, 0.0), false)
                .expect("circle arc")
        };
        let unit = full(0.0, 1.0);
        assert!(primitive_intersections(unit, full(0.0, 2.0)).is_empty());
        assert!(primitive_intersections(unit, full(3.0, 1.0)).is_empty());
        assert!(primitive_intersections(full(0.0, 3.0), full(1.0, 1.0)).is_empty());
        let crossing = primitive_intersections(unit, full(1.0, 1.0));
        assert_eq!(crossing.len(), 2);
        assert!(crossing.iter().all(|p| (p.x - 0.5).abs() < 1e-12));
        let tangent = primitive_intersections(unit, full(2.0, 1.0));
        assert!(tangent.iter().all(|p| (p.x - 1.0).abs() < 1e-12 && p.y.abs() < 1e-12));
        assert!(!tangent.is_empty());
        assert!(line_arc_intersections(point(2.0, 2.0), point(3.0, 2.0), unit).is_empty());
        assert!(line_arc_intersections(point(1.0, 0.0), point(1.0, 0.0), unit).is_empty());
        let line = Primitive::Line(point(-2.0, 0.0), point(2.0, 0.0));
        assert_eq!(primitive_intersections(line, unit).len(), 2);
        assert_eq!(primitive_intersections(unit, line).len(), 2);
        let quarter =
            arc(point(1.0, 0.0), point(0.0, 1.0), point(0.0, 0.0), false).expect("quarter arc");
        assert_eq!(primitive_intersections(line, quarter), vec![point(1.0, 0.0)]);
    }
}

pub(super) const MAX_COORDINATE: f64 = 1_000_000.0;
const MAX_RING_POINTS: usize = 2_048;
const MAX_SOURCE_POINTS: usize = 8_192;

#[derive(Debug, Clone, Copy)]
pub(super) struct Circle {
    pub center: Point2D,
    pub radius: f64,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedShape {
    pub geometry: MultiPolygon<f64>,
    pub boundary: MultiLineString<f64>,
    pub bounds: Rect<f64>,
    pub material_area: f64,
    pub circle: Option<Circle>,
    /// Minimum of the original source outline, before approximation offsets.
    pub original_min: Point2D,
}

#[derive(Clone, Copy)]
pub(super) enum Role {
    Bin,
    Item,
}

#[derive(Clone, Copy)]
enum Primitive {
    Line(Point2D, Point2D),
    Arc { start: Point2D, end: Point2D, center: Point2D, delta: f64 },
}

impl Primitive {
    fn endpoints(self) -> (Point2D, Point2D) {
        match self {
            Self::Line(start, end) | Self::Arc { start, end, .. } => (start, end),
        }
    }

    fn contains_angle(self, angle: f64) -> bool {
        let Self::Arc { start, center, delta, .. } = self else {
            return false;
        };
        let initial = (start.y - center.y).atan2(start.x - center.x);
        let distance = if delta < 0.0 {
            (initial - angle).rem_euclid(TAU)
        } else {
            (angle - initial).rem_euclid(TAU)
        };
        distance <= delta.abs() + 1e-12 || (TAU - distance).abs() <= 1e-12
    }
}

pub(super) fn constraint(field: &str, reason: impl Into<String>) -> BinPackingError {
    BinPackingError::InvalidConstraint { field: field.into(), reason: reason.into() }
}

fn checked_point(point: Point2D) -> Result<()> {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || point.x.abs() > MAX_COORDINATE
        || point.y.abs() > MAX_COORDINATE
    {
        return Err(constraint("shape", "coordinates must be finite and within +/-1000000"));
    }
    Ok(())
}

fn distance(a: Point2D, b: Point2D) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn close(a: Point2D, b: Point2D) -> bool {
    distance(a, b) <= 1e-9
}

fn arc(start: Point2D, end: Point2D, center: Point2D, clockwise: bool) -> Result<Primitive> {
    checked_point(start)?;
    checked_point(end)?;
    checked_point(center)?;
    let radius = distance(start, center);
    if radius <= 0.0
        || radius > MAX_COORDINATE
        || (radius - distance(end, center)).abs() > 1e-9 * radius.max(1.0)
    {
        return Err(constraint("arc", "endpoints must have the same positive finite radius"));
    }
    let initial = (start.y - center.y).atan2(start.x - center.x);
    let final_angle = (end.y - center.y).atan2(end.x - center.x);
    let magnitude = if start == end {
        TAU
    } else if clockwise {
        (initial - final_angle).rem_euclid(TAU)
    } else {
        (final_angle - initial).rem_euclid(TAU)
    };
    if magnitude == 0.0 {
        return Err(constraint("arc", "a non-closed arc must have positive angular length"));
    }
    Ok(Primitive::Arc { start, end, center, delta: if clockwise { -magnitude } else { magnitude } })
}

fn circle_points(
    center: Point2D,
    radius: f64,
    sweep: f64,
    initial: f64,
    tol: f64,
) -> Result<Vec<Point2D>> {
    // Each chord has at most tol/4 radial error. The subsequent 2*tol
    // envelope leaves room for the offset operation's 0.05-radian join chords.
    let step = 2.0 * (1.0 - (tol / (4.0 * radius)).min(0.5)).acos();
    let mut count = (sweep.abs() / step).ceil() as usize;
    count = count.max(1);
    if sweep.abs() >= TAU - 1e-12 {
        count = count.max(8).div_ceil(4).saturating_mul(4);
    }
    if count > MAX_RING_POINTS {
        return Err(constraint(
            "curve_tolerance",
            "circular contour needs more than 2048 chords; increase tolerance or rescale",
        ));
    }
    Ok((0..=count)
        .map(|index| {
            let angle = initial + sweep * index as f64 / count as f64;
            let (sin, cos) = angle.sin_cos();
            Point2D { x: center.x + radius * cos, y: center.y + radius * sin }
        })
        .collect())
}

fn line_arc_intersections(start: Point2D, end: Point2D, curved: Primitive) -> Vec<Point2D> {
    let Primitive::Arc { start: arc_start, center, .. } = curved else {
        return Vec::new();
    };
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let x = start.x - center.x;
    let y = start.y - center.y;
    let radius = distance(arc_start, center);
    let a = dx * dx + dy * dy;
    let b = 2.0 * (x * dx + y * dy);
    let c = x * x + y * y - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 || a == 0.0 {
        return Vec::new();
    }
    let root = discriminant.sqrt();
    [(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
        .into_iter()
        .filter(|&t| (0.0..=1.0).contains(&t))
        .map(|t| Point2D { x: start.x + t * dx, y: start.y + t * dy })
        .filter(|p| curved.contains_angle((p.y - center.y).atan2(p.x - center.x)))
        .collect()
}

fn primitive_intersections(first: Primitive, second: Primitive) -> Vec<Point2D> {
    match (first, second) {
        (Primitive::Line(a, b), Primitive::Line(c, d)) => {
            match line_intersection(
                Line::new(Coord { x: a.x, y: a.y }, Coord { x: b.x, y: b.y }),
                Line::new(Coord { x: c.x, y: c.y }, Coord { x: d.x, y: d.y }),
            ) {
                None => Vec::new(),
                Some(LineIntersection::SinglePoint { intersection, .. }) => {
                    vec![Point2D { x: intersection.x, y: intersection.y }]
                }
                Some(LineIntersection::Collinear { intersection }) => vec![
                    Point2D { x: intersection.start.x, y: intersection.start.y },
                    Point2D { x: intersection.end.x, y: intersection.end.y },
                ],
            }
        }
        (Primitive::Line(a, b), curved @ Primitive::Arc { .. })
        | (curved @ Primitive::Arc { .. }, Primitive::Line(a, b)) => {
            line_arc_intersections(a, b, curved)
        }
        (
            Primitive::Arc { start: a, center: c1, .. },
            Primitive::Arc { start: b, center: c2, .. },
        ) => {
            let r1 = distance(a, c1);
            let r2 = distance(b, c2);
            let separation = distance(c1, c2);
            if separation == 0.0 {
                if (r1 - r2).abs() > 1e-9 {
                    return Vec::new();
                }
                let mut points = Vec::new();
                for primitive in [first, second] {
                    let Primitive::Arc { start, end, center, delta } = primitive else {
                        continue;
                    };
                    let initial = (start.y - center.y).atan2(start.x - center.x);
                    let angle = initial + delta / 2.0;
                    let mid =
                        Point2D { x: center.x + r1 * angle.cos(), y: center.y + r1 * angle.sin() };
                    for p in [start, end, mid] {
                        let angle = (p.y - center.y).atan2(p.x - center.x);
                        if first.contains_angle(angle) && second.contains_angle(angle) {
                            points.push(p);
                        }
                    }
                }
                return points;
            }
            if separation > r1 + r2 || separation < (r1 - r2).abs() {
                return Vec::new();
            }
            let along = (r1 * r1 - r2 * r2 + separation * separation) / (2.0 * separation);
            let h2 = r1 * r1 - along * along;
            if h2 < -1e-9 * r1.max(1.0) {
                return Vec::new();
            }
            let height = h2.max(0.0).sqrt();
            let ux = (c2.x - c1.x) / separation;
            let uy = (c2.y - c1.y) / separation;
            [-1.0, 1.0]
                .into_iter()
                .map(|sign| Point2D {
                    x: c1.x + along * ux - sign * height * uy,
                    y: c1.y + along * uy + sign * height * ux,
                })
                .filter(|p| {
                    first.contains_angle((p.y - c1.y).atan2(p.x - c1.x))
                        && second.contains_angle((p.y - c2.y).atan2(p.x - c2.x))
                })
                .collect()
        }
    }
}

// O(S²) analytic contour validation for S original line/arc segments. Adjacent
// segments may meet at their shared endpoints but may not cross elsewhere.
fn validate_primitives(primitives: &[Primitive]) -> Result<()> {
    for (index, &first) in primitives.iter().enumerate() {
        for (other_index, &second) in primitives.iter().enumerate().skip(index + 1) {
            let neighboring =
                other_index == index + 1 || (index == 0 && other_index + 1 == primitives.len());
            let (a, b) = first.endpoints();
            let (c, d) = second.endpoints();
            let mut shared: Vec<_> = [a, b].into_iter().filter(|p| [c, d].contains(p)).collect();
            shared.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
            shared.dedup();
            let mut intersections = primitive_intersections(first, second);
            intersections.sort_by(|a, b| a.x.total_cmp(&b.x).then_with(|| a.y.total_cmp(&b.y)));
            intersections.dedup();
            if intersections.len() > shared.len() {
                return Err(constraint(
                    "shape",
                    "contour segments cross beyond their shared endpoints",
                ));
            }
            for point in intersections {
                let common_endpoint = [a, b].iter().any(|&first| {
                    close(point, first) && [c, d].iter().any(|&second| close(first, second))
                });
                if !neighboring || !common_endpoint {
                    return Err(constraint("shape", "contour segments self-intersect or overlap"));
                }
            }
        }
    }
    Ok(())
}

fn flatten(
    contour: &Contour2D,
    tolerance: f64,
    total_points: &mut usize,
) -> Result<(LineString<f64>, bool, f64, Point2D)> {
    let (primitives, curved) = match contour {
        Contour2D::Polygon { vertices } => {
            if vertices.len() < 3 || vertices.len() > MAX_RING_POINTS {
                return Err(constraint("shape", "polygon contours need 3..=2048 vertices"));
            }
            for &point in vertices {
                checked_point(point)?;
            }
            let points = if vertices.first() == vertices.last() {
                &vertices[..vertices.len() - 1]
            } else {
                vertices.as_slice()
            };
            if points.len() < 3 {
                return Err(constraint("shape", "polygon needs three distinct vertices"));
            }
            let mut primitives = Vec::new();
            for index in 0..points.len() {
                let start = points[index];
                let end = points[(index + 1) % points.len()];
                if start == end {
                    return Err(constraint("shape", "adjacent polygon vertices must differ"));
                }
                primitives.push(Primitive::Line(start, end));
            }
            (primitives, false)
        }
        Contour2D::Circle { center, radius } => {
            checked_point(*center)?;
            if !radius.is_finite() || *radius <= 0.0 || *radius > MAX_COORDINATE {
                return Err(constraint("circle.radius", "must be finite and in (0,1000000]"));
            }
            let start = Point2D { x: center.x + radius, y: center.y };
            checked_point(start)?;
            (vec![arc(start, start, *center, false)?], true)
        }
        Contour2D::Curve { start, segments } => {
            checked_point(*start)?;
            if segments.is_empty() || segments.len() > MAX_RING_POINTS {
                return Err(constraint("curve", "needs 1..=2048 segments"));
            }
            let mut current = *start;
            let mut primitives = Vec::new();
            for segment in segments {
                let primitive = match segment {
                    CurveSegment2D::Line { end } => {
                        checked_point(*end)?;
                        if current == *end {
                            return Err(constraint(
                                "curve",
                                "line segments must have positive length",
                            ));
                        }
                        Primitive::Line(current, *end)
                    }
                    CurveSegment2D::Arc { end, center, clockwise } => {
                        arc(current, *end, *center, *clockwise)?
                    }
                };
                current = primitive.endpoints().1;
                primitives.push(primitive);
            }
            if current != *start {
                return Err(constraint("curve", "last endpoint must close the contour"));
            }
            let curved = primitives.iter().any(|p| matches!(p, Primitive::Arc { .. }));
            (primitives, curved)
        }
    };
    validate_primitives(&primitives)?;
    let mut points = Vec::new();
    let mut area = 0.0;
    let origin = primitives[0].endpoints().0;
    let mut original_min = origin;
    for primitive in primitives {
        let (start, end) = primitive.endpoints();
        original_min.x = original_min.x.min(start.x).min(end.x);
        original_min.y = original_min.y.min(start.y).min(end.y);
        match primitive {
            Primitive::Line(_, _) => {
                points.push(start);
                area += (start.x - origin.x) * (end.y - origin.y)
                    - (end.x - origin.x) * (start.y - origin.y);
            }
            Primitive::Arc { center, delta, .. } => {
                let radius = distance(start, center);
                for (angle, dx, dy) in [
                    (0.0, 1.0, 0.0),
                    (std::f64::consts::FRAC_PI_2, 0.0, 1.0),
                    (std::f64::consts::PI, -1.0, 0.0),
                    (3.0 * std::f64::consts::FRAC_PI_2, 0.0, -1.0),
                ] {
                    if primitive.contains_angle(angle) {
                        let extremum =
                            Point2D { x: center.x + dx * radius, y: center.y + dy * radius };
                        checked_point(extremum)?;
                        original_min.x = original_min.x.min(extremum.x);
                        original_min.y = original_min.y.min(extremum.y);
                    }
                }
                let initial = (start.y - center.y).atan2(start.x - center.x);
                let mut sampled = circle_points(center, radius, delta, initial, tolerance)?;
                sampled.pop();
                points.extend(sampled);
                area += (center.x - origin.x) * (end.y - start.y)
                    - (center.y - origin.y) * (end.x - start.x)
                    + radius * radius * delta;
            }
        }
    }
    if points.len() < 3
        || points.len() > MAX_RING_POINTS
        || !area.is_finite()
        || area.abs() <= 1e-14
    {
        return Err(constraint(
            "shape",
            "contour area is degenerate or approximation exceeds 2048 vertices",
        ));
    }
    for &point in &points {
        checked_point(point)?;
    }
    *total_points = total_points.saturating_add(points.len());
    if *total_points > MAX_SOURCE_POINTS {
        return Err(constraint("shape", "prepared geometry exceeds 8192 source vertices"));
    }
    points.push(points[0]);
    Ok((
        LineString::from(points.into_iter().map(|point| (point.x, point.y)).collect::<Vec<_>>()),
        curved,
        area.abs() / 2.0,
        original_min,
    ))
}

pub(super) fn boundary(geometry: &MultiPolygon<f64>) -> MultiLineString<f64> {
    MultiLineString(
        geometry
            .0
            .iter()
            .flat_map(|polygon| {
                std::iter::once(polygon.exterior().clone())
                    .chain(polygon.interiors().iter().cloned())
            })
            .collect(),
    )
}

pub(super) fn prepare_shape(
    shape: &Shape2D,
    options: &IrregularOptions,
    role: Role,
) -> Result<PreparedShape> {
    if shape.components.is_empty() || shape.components.len() > 16 {
        return Err(constraint("shape.components", "must contain 1..=16 rigid components"));
    }
    let mut polygons = Vec::new();
    let mut curved = false;
    let mut material_area = 0.0;
    let mut points = 0usize;
    let mut original_min = Point2D { x: MAX_COORDINATE, y: MAX_COORDINATE };
    for component in &shape.components {
        if component.holes.len() > 32 {
            return Err(constraint("shape.holes", "at most 32 holes per component are supported"));
        }
        let (mut outer, has_curves, outer_area, source_min) =
            flatten(&component.exterior, options.curve_tolerance, &mut points)?;
        original_min.x = original_min.x.min(source_min.x);
        original_min.y = original_min.y.min(source_min.y);
        if Polygon::new(outer.clone(), Vec::new()).signed_area() < 0.0 {
            outer.0.reverse();
        }
        let mut holes = Vec::new();
        let mut area = outer_area;
        let mut component_curved = has_curves;
        for hole in &component.holes {
            let (mut ring, has_curves, hole_area, _) =
                flatten(hole, options.curve_tolerance, &mut points)?;
            if Polygon::new(ring.clone(), Vec::new()).signed_area() > 0.0 {
                ring.0.reverse();
            }
            area -= hole_area;
            component_curved |= has_curves;
            holes.push(ring);
        }
        let polygon = Polygon::new(outer, holes);
        polygon.check_validation().map_err(|error| constraint("shape", error.to_string()))?;
        if polygon
            .interiors()
            .iter()
            .any(|hole| Euclidean.distance(polygon.exterior(), hole) == 0.0)
        {
            return Err(constraint("shape", "holes must be strictly inside their exterior"));
        }
        if area <= 0.0 || !area.is_finite() {
            return Err(constraint("shape", "holes must leave positive material area"));
        }
        if component_curved {
            let bounds = polygon
                .bounding_rect()
                .ok_or_else(|| constraint("shape", "missing shape bounds"))?;
            let span = bounds.width().max(bounds.height());
            let magnitude = bounds
                .min()
                .x
                .abs()
                .max(bounds.min().y.abs())
                .max(bounds.max().x.abs())
                .max(bounds.max().y.abs());
            if options.curve_tolerance < (span * 1e-7).max(magnitude * 1e-9).max(1e-8) {
                return Err(constraint(
                    "curve_tolerance",
                    "too small for the coordinate scale; rescale or increase tolerance",
                ));
            }
            // When curves are near another ring, chord topology alone cannot
            // certify the source hole. Reject the uncertain input before offsetting.
            let rings: Vec<_> =
                std::iter::once(polygon.exterior()).chain(polygon.interiors()).collect();
            for (index, first) in rings.iter().enumerate() {
                for second in rings.iter().skip(index + 1) {
                    if Euclidean.distance(*first, *second) <= 4.0 * options.curve_tolerance {
                        return Err(constraint(
                            "curve_tolerance",
                            "curved ring separation is uncertain at this tolerance",
                        ));
                    }
                }
            }
        }
        curved |= component_curved;
        material_area += area;
        polygons.push(polygon);
    }
    let original = MultiPolygon(polygons);
    original
        .check_validation()
        .map_err(|error| constraint("shape.components", error.to_string()))?;
    if curved {
        for (index, first) in original.0.iter().enumerate() {
            for second in original.0.iter().skip(index + 1) {
                let first = boundary(&MultiPolygon(vec![first.clone()]));
                let second = boundary(&MultiPolygon(vec![second.clone()]));
                if Euclidean.distance(&first, &second) <= 4.0 * options.curve_tolerance {
                    return Err(constraint(
                        "curve_tolerance",
                        "component separation is uncertain at this tolerance",
                    ));
                }
            }
        }
    }
    let geometry = if curved {
        let distance = match role {
            Role::Bin => -2.0 * options.curve_tolerance,
            Role::Item => 2.0 * options.curve_tolerance,
        };
        original.buffer_with_style(BufferStyle::new(distance).line_join(LineJoin::Round(0.05)))
    } else {
        original
    };
    if geometry.0.is_empty()
        || !geometry.is_valid()
        || !geometry.coords_iter().all(|p| p.x.is_finite() && p.y.is_finite())
    {
        return Err(constraint("curve_tolerance", "conservative envelope is empty or invalid"));
    }
    let bounds =
        geometry.bounding_rect().ok_or_else(|| constraint("shape", "missing envelope bounds"))?;
    let circle = if shape.components.len() == 1 && shape.components[0].holes.is_empty() {
        match shape.components[0].exterior {
            Contour2D::Circle { center, radius } => Some(Circle { center, radius }),
            _ => None,
        }
    } else {
        None
    };
    Ok(PreparedShape {
        boundary: boundary(&geometry),
        geometry,
        bounds,
        material_area,
        circle,
        original_min,
    })
}

pub(super) fn transform(
    shape: &PreparedShape,
    angle: f64,
    mirrored: bool,
    x: f64,
    y: f64,
) -> PreparedShape {
    let (sin, cos) = if (angle / 90.0).fract() == 0.0 {
        match (angle as u32 / 90) % 4 {
            0 => (0.0, 1.0),
            1 => (1.0, 0.0),
            2 => (0.0, -1.0),
            _ => (-1.0, 0.0),
        }
    } else {
        angle.to_radians().sin_cos()
    };
    let apply = |point: Coord<f64>| {
        let px = if mirrored { -point.x } else { point.x };
        Coord { x: px * cos - point.y * sin + x, y: px * sin + point.y * cos + y }
    };
    let geometry = shape.geometry.map_coords(apply);
    let bounds = geometry.bounding_rect().unwrap_or(shape.bounds);
    let circle = shape.circle.map(|circle| {
        let center = apply(Coord { x: circle.center.x, y: circle.center.y });
        Circle { center: Point2D { x: center.x, y: center.y }, radius: circle.radius }
    });
    PreparedShape {
        boundary: boundary(&geometry),
        geometry,
        bounds,
        material_area: shape.material_area,
        circle,
        original_min: shape.original_min,
    }
}

pub(super) fn actual_bounds(shape: &PreparedShape) -> Rect<f64> {
    match shape.circle {
        Some(circle) => Rect::new(
            (circle.center.x - circle.radius, circle.center.y - circle.radius),
            (circle.center.x + circle.radius, circle.center.y + circle.radius),
        ),
        None => shape.bounds,
    }
}

pub(super) fn overlaps(first: &PreparedShape, second: &PreparedShape, spacing: f64) -> bool {
    if let (Some(a), Some(b)) = (first.circle, second.circle) {
        return distance(a.center, b.center) < a.radius + b.radius + spacing;
    }
    if first.geometry.relate(&second.geometry).get(CoordPos::Inside, CoordPos::Inside)
        == Dimensions::TwoDimensional
    {
        return true;
    }
    spacing > 0.0 && Euclidean.distance(&first.geometry, &second.geometry) < spacing
}

pub(super) fn covered_by(item: &PreparedShape, bin: &PreparedShape, spacing: f64) -> bool {
    if let (Some(item), Some(bin)) = (item.circle, bin.circle) {
        return distance(item.center, bin.center) + item.radius + spacing <= bin.radius;
    }
    bin.geometry.relate(&item.geometry).is_covers()
        && (spacing == 0.0 || Euclidean.distance(&bin.boundary, &item.geometry) >= spacing)
}

pub(super) fn anchor_vertices(shape: &PreparedShape) -> (Vec<Point2D>, bool) {
    let mut points: Vec<_> =
        shape.geometry.coords_iter().map(|coord| Point2D { x: coord.x, y: coord.y }).collect();
    points.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));
    points.dedup_by(|a, b| a == b);
    // The geometric predicate still sees every vertex. Contact enumeration is a
    // bounded heuristic; retain evenly sampled contour points plus all bounds.
    let sampled = points.len() > 64;
    if sampled {
        let stride = points.len().div_ceil(60);
        points = points.into_iter().step_by(stride).collect();
    }
    let bounds = actual_bounds(shape);
    points.extend([
        Point2D { x: bounds.min().x, y: bounds.min().y },
        Point2D { x: bounds.max().x, y: bounds.min().y },
        Point2D { x: bounds.min().x, y: bounds.max().y },
        Point2D { x: bounds.max().x, y: bounds.max().y },
    ]);
    (points, sampled)
}

pub(super) fn rectangle_center(bounds: Rect<f64>) -> Point2D {
    Point2D {
        x: (bounds.min().x + bounds.max().x) / 2.0,
        y: (bounds.min().y + bounds.max().y) / 2.0,
    }
}
