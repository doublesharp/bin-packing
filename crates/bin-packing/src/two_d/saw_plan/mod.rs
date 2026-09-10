//! Opt-in workpiece-aware table/panel saw scheduling.
//!
//! Use [`plan_saw_cuts`] with explicit stock and demand provenance. Legacy
//! [`super::cut_plan::plan_cuts`] remains unchanged. See `docs/two-d/saw-planner.md`
//! for machine assumptions, candidate-search scope and Subchecker migration.

mod geometry;
mod model;
mod search;

pub use model::*;

/// Build a physical job schedule for existing layouts without changing packing.
///
/// Initial accessory setups are counted and scored separately from subsequent
/// changes. Machine state survives rotations, workpiece loading and sheet changes.
/// Sheet views reference this job and must not be executed independently.
///
/// # Errors
/// Returns [`SawPlanError2D`] for invalid mappings, dimensions, overlaps, rotation
/// violations, unsupported options, non-guillotine geometry or baseline exhaustion.
/// Original problem constraints are validated before geometry or search.
pub fn plan_saw_cuts(
    job: &SawJob2D,
    options: &SawPlanOptions2D,
) -> Result<SawPlan2D, SawPlanError2D> {
    geometry::validate(job, options)?;
    search::plan(job, options)
}
