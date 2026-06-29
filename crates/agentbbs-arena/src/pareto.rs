//! Pareto-frontier dominance for the Retort track.
//!
//! The two competing objectives are **accuracy** (`requirement_coverage`,
//! maximize) and **cost** (`$/task`, minimize). A stack is *dominated* when
//! another stack is at least as accurate **and** at least as cheap, strictly
//! better on at least one axis. The set of non-dominated stacks is the
//! **frontier** (tier 1); peeling it off and repeating yields the Pareto tier
//! of every stack (non-dominated sorting, à la NSGA-II).
//!
//! This mirrors Retort's `pareto_analysis` (wrapped by retort-metaharness'
//! `report.py`): the dominance relation is identical, so the Arena's recomputed
//! frontier agrees with the ingested one point-for-point. When a results bundle
//! carries report.py's frontier, [`crate::retort`] cross-checks against it.

/// A point in the accuracy-vs-cost plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParetoPoint {
    /// Accuracy (requirement_coverage), higher is better.
    pub coverage: f64,
    /// Cost ($/task), lower is better.
    pub cost: f64,
}

/// Float comparison slack so equal values aren't spuriously "strictly better".
const EPS: f64 = 1e-9;

/// Whether `a` dominates `b`: at least as accurate and at least as cheap, and
/// strictly better on at least one axis.
pub fn dominates(a: ParetoPoint, b: ParetoPoint) -> bool {
    let not_worse = a.coverage >= b.coverage - EPS && a.cost <= b.cost + EPS;
    let strictly_better = a.coverage > b.coverage + EPS || a.cost < b.cost - EPS;
    not_worse && strictly_better
}

/// Non-dominated-sorting tier per input index (1 = frontier, 2 = next frontier
/// after removing tier 1, …). Deterministic and order-independent.
pub fn nondominated_tiers(points: &[ParetoPoint]) -> Vec<u32> {
    let n = points.len();
    let mut tier = vec![0u32; n];
    let mut remaining: Vec<usize> = (0..n).collect();
    let mut current = 1u32;
    while !remaining.is_empty() {
        let front: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|&i| {
                !remaining
                    .iter()
                    .any(|&j| j != i && dominates(points[j], points[i]))
            })
            .collect();
        // With a strict (irreflexive) dominance relation the frontier is never
        // empty while points remain; guard anyway so we always terminate.
        let front = if front.is_empty() {
            remaining.clone()
        } else {
            front
        };
        for &i in &front {
            tier[i] = current;
        }
        remaining.retain(|i| !front.contains(i));
        current += 1;
    }
    tier
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(coverage: f64, cost: f64) -> ParetoPoint {
        ParetoPoint { coverage, cost }
    }

    #[test]
    fn dominance_basic() {
        // cheaper AND more accurate dominates.
        assert!(dominates(p(0.9, 0.1), p(0.8, 0.2)));
        // same accuracy, cheaper dominates.
        assert!(dominates(p(0.9, 0.1), p(0.9, 0.5)));
        // more accurate but pricier does NOT dominate (a tradeoff).
        assert!(!dominates(p(0.95, 0.5), p(0.8, 0.1)));
        // identical points don't dominate each other.
        assert!(!dominates(p(0.9, 0.1), p(0.9, 0.1)));
    }

    #[test]
    fn frontier_and_dominated_tiers() {
        // The expensive high-accuracy baseline is dominated by a cheaper stack
        // with equal/higher accuracy — the cost-lever story.
        let pts = vec![
            p(0.94, 0.085), // 0 frontier (cheap + most accurate among cheap)
            p(0.935, 0.50), // 1 baseline: dominated by 0
            p(0.85, 0.041), // 2 frontier
            p(0.675, 0.012),// 3 frontier
            p(0.525, 0.006),// 4 frontier (cheapest)
        ];
        let tiers = nondominated_tiers(&pts);
        assert_eq!(tiers, vec![1, 2, 1, 1, 1]);
    }
}
