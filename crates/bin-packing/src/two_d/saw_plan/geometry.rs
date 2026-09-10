use super::*;
use crate::two_d::Placement2D;

pub(super) fn validate(job: &SawJob2D, options: &SawPlanOptions2D) -> Result<(), SawPlanError2D> {
    let w = options.weights;
    for (field, value) in [
        ("cut", w.cut),
        ("initial_setup", w.initial_setup),
        ("setup_change", w.setup_change),
        ("rotation", w.rotation),
        ("handling", w.handling),
    ] {
        // The input cap and split-depth cap keep aggregate scores finite at this bound.
        if !value.is_finite() || !(0.0..=1e100).contains(&value) {
            return Err(SawPlanError2D::InvalidOption { field });
        }
    }
    let limits = options.search;
    if limits.max_states > 100_000
        || !(1..=256).contains(&limits.beam_width)
        || !(1..=100_000).contains(&limits.baseline_max_states)
    {
        return Err(SawPlanError2D::InvalidOption { field: "search" });
    }
    if job.layouts.len() > 128
        || job.layouts.iter().map(|l| l.placements.len()).sum::<usize>() > 512
    {
        return Err(SawPlanError2D::InvalidOption {
            field: "job exceeds 128 sheets or 512 placements",
        });
    }
    // The legacy solver rejects duplicate stock labels. This API identifies stocks by
    // index, so normalize only the validation copy and preserve all physical constraints.
    let mut problem = job.problem.clone();
    for (i, stock) in problem.sheets.iter_mut().enumerate() {
        stock.name = i.to_string();
    }
    problem.validate().map_err(|e| SawPlanError2D::InvalidProblem(e.to_string()))?;
    if job.stock_indices.len() != job.layouts.len() || job.demand_indices.len() != job.layouts.len()
    {
        return Err(mapping(job.layouts.len(), "one stock and demand map is required per layout"));
    }
    let mut stock_counts = vec![0usize; problem.sheets.len()];
    let mut demand_counts = vec![0usize; problem.demands.len()];
    for (sheet_index, layout) in job.layouts.iter().enumerate() {
        let stock_index = job.stock_indices[sheet_index];
        let stock = problem
            .sheets
            .get(stock_index)
            .ok_or_else(|| mapping(sheet_index, "stock index out of range"))?;
        stock_counts[stock_index] += 1;
        if stock.quantity.is_some_and(|q| stock_counts[stock_index] > q) {
            return Err(mapping(sheet_index, "stock inventory exceeded"));
        }
        if let Some(&first) = job.stock_indices.first()
            && problem.sheets.get(first).is_some_and(|s| s.kerf != stock.kerf)
        {
            return Err(mapping(sheet_index, "mixed blade kerfs require separate jobs"));
        }
        if layout.width != stock.width || layout.height != stock.height {
            return Err(mapping(sheet_index, "layout dimensions differ from source stock"));
        }
        if job.demand_indices[sheet_index].len() != layout.placements.len() {
            return Err(mapping(sheet_index, "one demand index is required per placement"));
        }
        for (i, p) in layout.placements.iter().enumerate() {
            let demand_index = job.demand_indices[sheet_index][i];
            let d = problem
                .demands
                .get(demand_index)
                .ok_or_else(|| mapping(sheet_index, "demand index out of range"))?;
            demand_counts[demand_index] += 1;
            if demand_counts[demand_index] > d.quantity {
                return Err(mapping(sheet_index, "demand quantity exceeded"));
            }
            let dimensions = if p.rotated { (d.height, d.width) } else { (d.width, d.height) };
            if (p.width, p.height) != dimensions || (p.rotated && !d.can_rotate) {
                return Err(mapping(
                    sheet_index,
                    "placement dimensions or rotation violate demand",
                ));
            }
            if u64::from(p.x) + u64::from(p.width) > u64::from(stock.width)
                || u64::from(p.y) + u64::from(p.height) > u64::from(stock.height)
            {
                return Err(SawPlanError2D::InvalidGeometry {
                    sheet_index,
                    reason: "finished part extends beyond physical stock",
                });
            }
            for other in &layout.placements[..i] {
                if overlap(rect(p), rect(other)) {
                    return Err(SawPlanError2D::InvalidGeometry {
                        sheet_index,
                        reason: "finished parts overlap",
                    });
                }
            }
        }
    }
    Ok(())
}

fn mapping(sheet_index: usize, reason: &'static str) -> SawPlanError2D {
    SawPlanError2D::InvalidMapping { sheet_index, reason }
}

pub(super) fn rect(p: &Placement2D) -> SawRect2D {
    SawRect2D { x: p.x, y: p.y, width: p.width, height: p.height }
}

fn overlap(a: SawRect2D, b: SawRect2D) -> bool {
    a.x < b.x.saturating_add(b.width)
        && b.x < a.x.saturating_add(a.width)
        && a.y < b.y.saturating_add(b.height)
        && b.y < a.y.saturating_add(a.height)
}

/// One ready physical workpiece. Part indices borrow the shared input placements.
#[derive(Debug, Clone)]
pub(super) struct Piece {
    pub id: WorkpieceId2D,
    pub parts: Vec<usize>,
    pub orientation: u8,
    pub created_by: Option<usize>,
}

impl Piece {
    pub fn finished(&self, job: &SawJob2D) -> Option<PartId2D> {
        if self.parts.len() == 1
            && rect(&job.layouts[self.id.sheet_index].placements[self.parts[0]]) == self.id.bounds
        {
            Some(PartId2D { sheet_index: self.id.sheet_index, placement_index: self.parts[0] })
        } else {
            None
        }
    }
    pub fn terminal(&self, job: &SawJob2D) -> bool {
        self.parts.is_empty() || self.finished(job).is_some()
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Cut {
    pub axis: SawAxis2D,
    pub band: SawRect2D,
}

#[derive(Debug, Clone)]
pub(super) struct Split {
    pub cut: Cut,
    pub children: Vec<Piece>,
}

/// O(p log p) boundary enumeration with O(p) lightweight descriptors. Feasibility
/// is checked lazily so baseline construction need not partition every alternative.
pub(super) fn cut_candidates(job: &SawJob2D, piece: &Piece) -> Vec<Cut> {
    let b = piece.id.bounds;
    let sheet = piece.id.sheet_index;
    let stock = &job.problem.sheets[job.stock_indices[sheet]];
    let placements = &job.layouts[sheet].placements;
    let mut result = Vec::new();
    for axis in [SawAxis2D::Vertical, SawAxis2D::Horizontal] {
        let vertical = axis == SawAxis2D::Vertical;
        let (lo, length) = if vertical { (b.x, b.width) } else { (b.y, b.height) };
        let hi = lo.saturating_add(length);
        let mut candidates = Vec::with_capacity(piece.parts.len().saturating_mul(2));
        for &i in &piece.parts {
            let p = &placements[i];
            let (start, size) = if vertical { (p.x, p.width) } else { (p.y, p.height) };
            candidates.push(i64::from(start) - i64::from(stock.kerf));
            candidates.push(i64::from(start) + i64::from(size));
        }
        candidates.sort_unstable();
        candidates.dedup();
        for blade_start in candidates {
            let blade_end = blade_start + i64::from(stock.kerf);
            if !stock.edge_kerf_relief && (blade_start < i64::from(lo) || blade_end > i64::from(hi))
            {
                continue;
            }
            let start = blade_start.max(i64::from(lo));
            let end = blade_end.min(i64::from(hi));
            if start > end || start >= i64::from(hi) || end <= i64::from(lo) {
                continue;
            }
            let start = start as u32;
            let end = end as u32;
            result.push(Cut {
                axis,
                band: if vertical {
                    SawRect2D { x: start, width: end - start, ..b }
                } else {
                    SawRect2D { y: start, height: end - start, ..b }
                },
            });
        }
    }
    result
}

/// O(p) feasibility/partition of one descriptor. A nonintrusive split can still
/// strand a narrow margin downstream, so baseline construction must backtrack.
pub(super) fn split_at(job: &SawJob2D, piece: &Piece, cut: Cut) -> Option<Split> {
    let b = piece.id.bounds;
    let sheet = piece.id.sheet_index;
    let vertical = cut.axis == SawAxis2D::Vertical;
    let (lo, hi, start, end) = if vertical {
        (b.x, b.x.saturating_add(b.width), cut.band.x, cut.band.x.saturating_add(cut.band.width))
    } else {
        (b.y, b.y.saturating_add(b.height), cut.band.y, cut.band.y.saturating_add(cut.band.height))
    };
    let mut low_parts = Vec::new();
    let mut high_parts = Vec::new();
    for &i in &piece.parts {
        let p = &job.layouts[sheet].placements[i];
        let (p_lo, size) = if vertical { (p.x, p.width) } else { (p.y, p.height) };
        if p_lo.saturating_add(size) <= start {
            low_parts.push(i);
        } else if p_lo >= end {
            high_parts.push(i);
        } else {
            return None;
        }
    }
    let mut children = Vec::with_capacity(2);
    for (child_lo, child_hi, parts) in [(lo, start, low_parts), (end, hi, high_parts)] {
        if child_hi <= child_lo {
            continue;
        }
        let bounds = if vertical {
            SawRect2D { x: child_lo, width: child_hi - child_lo, ..b }
        } else {
            SawRect2D { y: child_lo, height: child_hi - child_lo, ..b }
        };
        children.push(Piece {
            id: WorkpieceId2D { sheet_index: sheet, bounds },
            parts,
            orientation: piece.orientation,
            created_by: None,
        });
    }
    Some(Split { cut, children })
}

/// O(p²) full candidate expansion for beam search; results are cached across
/// machine states. Baseline construction uses the lazy descriptor API instead.
pub(super) fn splits(job: &SawJob2D, piece: &Piece) -> Vec<Split> {
    cut_candidates(job, piece).into_iter().filter_map(|cut| split_at(job, piece, cut)).collect()
}
