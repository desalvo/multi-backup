//! Native reliability-policy core for the staged Python -> Rust migration.
//!
//! Python still provides the working GUI, encrypted format and providers until
//! differential compatibility tests and all native E2E workflows are certified.
//! This Rust module intentionally does not open or modify existing archives.

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

#[cfg(test)]
mod tests {
    use super::{BackfillPlanner, RunOutcome};

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
