//! Native reliability-policy core for the staged Python -> Rust migration.
//!
//! Python still provides the working GUI, encrypted format and providers until
//! differential compatibility tests and all native E2E workflows are certified.
//! This Rust module intentionally does not open or modify existing archives.

pub mod archive;
pub mod archive_management;
pub mod checkpoint_audit;
pub mod journal;
pub mod remote_restore;
pub mod dropbox_hash;
pub mod dropbox_client;
pub mod github_client;
pub mod vault;

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Complete,
    Partial,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiSummary {
    pub target: usize,
    pub complete: usize,
    pub attempted: usize,
    pub unsuccessful: Vec<u64>,
    pub shortfall: usize,
    pub warnings: bool,
    pub may_finish: bool,
}

/// A candidate scheduler. Its Rust policy mirrors the deployed Python
/// algorithm: skip unsuccessful CI, backfill older eligible runs, never turn a
/// partially archived run into a successful or verified complete run.
#[derive(Debug)]
pub struct BackfillPlanner {
    candidates: Vec<u64>,
    next: usize,
    target: usize,
    completed: usize,
    examined: usize,
    unsuccessful: Vec<u64>,
    ignore_errors: bool,
}

impl BackfillPlanner {
    pub fn new(candidate_ids: impl IntoIterator<Item = u64>,
               max_runs: usize, ignore_errors: bool) -> Self {
        let mut seen = HashSet::new();
        let candidates: Vec<u64> = candidate_ids.into_iter()
            .filter(|id| *id != 0 && seen.insert(*id)).collect();
        let target = if max_runs == 0 {
            candidates.len()
        } else {
            max_runs.min(candidates.len())
        };
        Self {
            candidates, next: 0, target, completed: 0, examined: 0,
            unsuccessful: Vec::new(), ignore_errors,
        }
    }

    pub fn next_candidate(&mut self) -> Option<u64> {
        if self.completed >= self.target {
            return None;
        }
        let id = *self.candidates.get(self.next)?;
        self.next += 1;
        Some(id)
    }

    pub fn record(&mut self, id: u64, outcome: RunOutcome) {
        self.examined += 1;
        match outcome {
            RunOutcome::Complete => self.completed += 1,
            RunOutcome::Partial | RunOutcome::Failed => self.unsuccessful.push(id),
        }
    }

    pub fn summary(&self) -> CiSummary {
        let shortfall = self.target.saturating_sub(self.completed);
        CiSummary {
            target: self.target,
            complete: self.completed,
            attempted: self.examined,
            unsuccessful: self.unsuccessful.clone(),
            shortfall,
            warnings: !self.unsuccessful.is_empty(),
            may_finish: shortfall == 0 || self.ignore_errors,
        }
    }
}

/// Canonical numeric progress mapping in the native, dependency-free core.
/// HSV: 0% red, 50% yellow, 100% green. No filesystem or credential access.
pub fn progress_rgb(percent: f64) -> (u8, u8, u8) {
    let p = if percent.is_finite() { percent.clamp(0.0, 100.0) }
            else { 0.0 };
    let s = 0.86_f64;
    let v = 0.91_f64;
    let chroma = v * s;
    let hue = p / 100.0 * 120.0;
    let x = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = v - chroma;
    let (r, g, b) = if hue < 60.0 {
        (chroma, x, 0.0)
    } else {
        (x, chroma, 0.0)
    };
    (((r + m) * 255.0).round() as u8,
     ((g + m) * 255.0).round() as u8,
     ((b + m) * 255.0).round() as u8)
}

/// A compact stable ABI for the Python/Tk transition.
#[no_mangle]
pub extern "C" fn multi_backup_progress_rgb(percent: f64) -> u32 {
    let (r, g, b) = progress_rgb(percent);
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[no_mangle]
pub extern "C" fn multi_backup_completion_percent(completed: u64, total: u64) -> u32 {
    if total == 0 { return 0; }
    let fraction = (completed as f64) / (total as f64);
    (fraction * 100.0).clamp(0.0, 100.0).round() as u32
}

/// Preserve the exact deployed GitHub Actions candidate/backfill limits.
#[no_mangle]
pub extern "C" fn multi_backup_ci_target(available: u64, requested: u64) -> u64 {
    if requested == 0 { available } else { available.min(requested) }
}

/// No unexpected extra CI downloads: schedule only when both capacity and
/// target slots remain; saturating arithmetic avoids user-controlled overflow.
#[no_mangle]
pub extern "C" fn multi_backup_ci_should_schedule(
    completed: u64, pending: u64, target: u64, worker_capacity: u64
) -> u32 {
    if worker_capacity > 0 && pending < worker_capacity &&
       completed.saturating_add(pending) < target { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::{BackfillPlanner, RunOutcome, progress_rgb,
                multi_backup_progress_rgb, multi_backup_completion_percent,
                multi_backup_ci_target, multi_backup_ci_should_schedule};

    #[test]
    fn progress_kernel_endpoints_are_red_yellow_green() {
        assert_eq!(progress_rgb(0.0), (232, 32, 32));
        assert_eq!(progress_rgb(50.0), (232, 232, 32));
        assert_eq!(progress_rgb(100.0), (32, 232, 32));
        assert_eq!(progress_rgb(f64::NAN), progress_rgb(0.0));
        assert_eq!(progress_rgb(-200.0), progress_rgb(0.0));
        assert_eq!(progress_rgb(200.0), progress_rgb(100.0));
        assert_eq!(multi_backup_progress_rgb(0.0), 0xE82020);
    }

    #[test]
    fn progress_events_do_not_fabricate_completion() {
        assert_eq!(multi_backup_completion_percent(0, 0), 0);
        assert_eq!(multi_backup_completion_percent(0, 100), 0);
        assert_eq!(multi_backup_completion_percent(50, 100), 50);
        assert_eq!(multi_backup_completion_percent(100, 100), 100);
        assert_eq!(multi_backup_completion_percent(110, 100), 100);
    }

    #[test]
    fn ci_scheduler_mirrors_conservative_python_backfill() {
        assert_eq!(multi_backup_ci_target(7, 0), 7);
        assert_eq!(multi_backup_ci_target(7, 5), 5);
        assert_eq!(multi_backup_ci_target(2, 5), 2);
        assert_eq!(multi_backup_ci_should_schedule(1, 2, 5, 4), 1);
        assert_eq!(multi_backup_ci_should_schedule(3, 2, 5, 4), 0);
        assert_eq!(multi_backup_ci_should_schedule(0, 4, 5, 4), 0);
        assert_eq!(multi_backup_ci_should_schedule(u64::MAX, 1, 4, 4), 0);
        assert_eq!(multi_backup_ci_should_schedule(0, 0, 5, 0), 0);
    }

    #[test]
    fn older_ci_replaces_broken_recent_artifact() {
        let mut planner = BackfillPlanner::new([11, 10, 9, 8, 7], 3, false);
        for (id, state) in [
            (11, RunOutcome::Complete),
            (10, RunOutcome::Failed),
            (9, RunOutcome::Complete),
            (8, RunOutcome::Complete),
        ] {
            assert_eq!(planner.next_candidate(), Some(id));
            planner.record(id, state);
        }
        assert_eq!(planner.next_candidate(), None);
        assert_eq!(planner.summary().complete, 3);
        assert_eq!(planner.summary().shortfall, 0);
        assert_eq!(planner.summary().unsuccessful, vec![10]);
        assert!(planner.summary().may_finish);
    }

    #[test]
    fn strict_shortfall_blocks_full_completion() {
        let mut planner = BackfillPlanner::new([11, 10], 2, false);
        for (id, state) in [(11, RunOutcome::Failed), (10, RunOutcome::Partial)] {
            assert_eq!(planner.next_candidate(), Some(id));
            planner.record(id, state);
        }
        assert_eq!(planner.next_candidate(), None);
        assert_eq!(planner.summary().shortfall, 2);
        assert!(!planner.summary().may_finish);
    }

    #[test]
    fn opted_in_shortfall_keeps_warnings() {
        let mut planner = BackfillPlanner::new([11, 10], 2, true);
        for id in [11, 10] {
            assert_eq!(planner.next_candidate(), Some(id));
            planner.record(id, RunOutcome::Partial);
        }
        assert!(planner.summary().may_finish);
        assert!(planner.summary().warnings);
        assert_eq!(planner.summary().complete, 0);
    }

    #[test]
    fn duplicate_zero_and_all_limit_handled() {
        let mut planner = BackfillPlanner::new([3, 0, 3, 2], 0, false);
        assert_eq!(planner.summary().target, 2);
        assert_eq!(planner.next_candidate(), Some(3));
        planner.record(3, RunOutcome::Complete);
        assert_eq!(planner.next_candidate(), Some(2));
        planner.record(2, RunOutcome::Complete);
        assert_eq!(planner.next_candidate(), None);
        assert!(!planner.summary().warnings);
    }
}
