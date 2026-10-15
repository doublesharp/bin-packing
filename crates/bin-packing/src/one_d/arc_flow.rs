//! Bounded multi-stock integer search over compressed pattern DAGs.

use std::collections::HashMap;
use std::mem::size_of;

use super::model::{PackedBin, PieceInstance};
use super::{OneDAlgorithm, OneDOptions, OneDProblem, OneDSolution, SolverMetrics1D, Stock1D};
use crate::{BinPackingError, Result};

const MAX_TYPES: usize = 128;
const MAX_DEPTH: usize = 128;
const MAX_INPUT_QUANTITY: usize = 4096;

/// Solve a cutting-stock problem with compressed per-stock pattern DAGs.
///
/// The integer search minimizes unplaced cuts, then bar count, unused usable
/// length, and cost, matching [`super::solve_1d`]. Demand rows remain separate,
/// including rows with repeated names. Kerf, trim and finite inventory are supported.
/// This is an exponential integer search, not a polynomial min-cost-flow solver.
///
/// `options.algorithm` is ignored. An Auto solution supplies the initial incumbent
/// and relaxed-inventory procurement estimates. `exact_pattern_limit` bounds the
/// combined DAG node and arc count. The integer search spends at most
/// `exact_pattern_limit * max(column_generation_rounds, 1)` work units on states
/// and arc visits. At most 128 demand types, 128 stock types and 128 selected bars
/// are searched. Inputs above 128 demand or stock types, or 4,096 total cuts,
/// are rejected before constructing the incumbent. Hitting a graph, search or
/// selected-bar limit retains the best incumbent with `exact = false`.
/// `exact = true` means the entire supported integer search completed.
///
/// DAG paths are streamed; a pool of all patterns is never materialized. Metrics
/// notes report graph size, saturating represented-path counts, storage estimates,
/// search work and any truncation. `enumerated_patterns` counts streamed patterns
/// across all demand states, so it may count the same pattern more than once.
/// The memo table stores at most `exact_pattern_limit` records. It is disabled
/// for costs whose sums are not exactly representable integers, preserving the
/// public floating-point cost comparison. Incumbent and procurement construction
/// use the existing Auto solver and are outside the graph/search work budgets.
/// Procurement quantities remain estimates from the existing relaxed Auto solve.
///
/// # Errors
/// Returns the same input validation and feasibility errors as [`super::solve_1d`],
/// or [`BinPackingError::Unsupported`] when an input cardinality limit is exceeded.
pub fn solve_1d_arc_flow(problem: OneDProblem, options: OneDOptions) -> Result<OneDSolution> {
    problem.validate()?;
    if problem.demands.len() > MAX_TYPES
        || problem.stock.len() > MAX_TYPES
        || problem.total_quantity() > MAX_INPUT_QUANTITY
    {
        return Err(BinPackingError::Unsupported(
            "arc-flow supports at most 128 demand types, 128 stock types and 4096 total cuts"
                .into(),
        ));
    }
    problem.ensure_feasible_demands()?;
    let incumbent_options = OneDOptions { algorithm: OneDAlgorithm::Auto, ..options.clone() };
    let mut incumbent = super::solve_1d(problem.clone(), incumbent_options)?;
    let required: Vec<_> =
        incumbent.stock_requirements.iter().map(|r| r.required_quantity).collect();
    incumbent.algorithm = "arc_flow".into();
    incumbent.exact = false;
    incumbent.lower_bound = None;

    let mut graph_budget = Budget::new(options.exact_pattern_limit);
    let mut graphs = Vec::new();
    let mut stats = GraphStats::default();
    let mut reason = None;
    for stock in &problem.stock {
        if let Some(graph) = PatternDag::build(&problem, stock, &mut graph_budget, &mut stats) {
            graphs.push(graph);
        } else {
            reason = Some("graph budget");
            break;
        }
    }
    // Integer additions up to 2^53 are exact in f64. For other costs, retain all
    // prefixes: layout sorting can change floating-point summation order.
    let memoize = problem.stock.iter().all(|stock| {
        stock.cost.fract() == 0.0
            && stock.cost <= (1_u64 << 53) as f64
            && u128::from(stock.cost as u64) * problem.total_quantity() as u128 <= (1_u128 << 53)
    });

    let mut search = Search {
        problem: &problem,
        graphs: &graphs,
        incumbent,
        remaining: problem.demands.iter().map(|d| d.quantity).collect(),
        usage: vec![0; problem.stock.len()],
        path: Vec::new(),
        waste: 0,
        memo: HashMap::new(),
        memo_limit: if memoize { options.exact_pattern_limit } else { 0 },
        budget: Budget::new(
            options.exact_pattern_limit.saturating_mul(options.column_generation_rounds.max(1)),
        ),
        states: 0,
        patterns: 0,
        truncated: reason,
    };
    if search.truncated.is_none() {
        search.visit()?;
    }
    let exact = search.truncated.is_none();
    let notes = vec![
        format!(
            "arc-flow graph_nodes={} graph_arcs={} represented_patterns={} graph_storage_bytes={} (saturating path count; storage excludes allocator and build-map overhead)",
            stats.nodes, stats.arcs, stats.patterns, stats.bytes
        ),
        format!(
            "arc-flow search_work={} search_states={} memo_states={} streamed_patterns={}",
            search.budget.spent,
            search.states,
            search.memo.len(),
            search.patterns
        ),
        match search.truncated {
            Some(reason) => format!(
                "arc-flow truncated: {reason}; retained best incumbent; no optimality certificate"
            ),
            None => "arc-flow complete integer search; lexicographic optimum certified".into(),
        },
    ];
    let mut solution = search.incumbent;
    solution.exact = exact;
    solution.metrics = SolverMetrics1D {
        iterations: 1,
        generated_patterns: 0,
        enumerated_patterns: search.patterns,
        explored_states: search.states,
        notes,
    };
    if problem.stock.iter().any(|s| s.available.is_some()) {
        solution.set_required_stock_counts(&problem.stock, &required);
        solution
            .metrics
            .notes
            .push("stock requirements estimated from a relaxed-inventory auto solve".into());
    }
    Ok(solution)
}

#[derive(Debug)]
struct Budget {
    limit: usize,
    spent: usize,
}

impl Budget {
    fn new(limit: usize) -> Self {
        Self { limit, spent: 0 }
    }

    fn spend(&mut self) -> bool {
        if self.spent >= self.limit {
            return false;
        }
        self.spent += 1;
        true
    }
}

#[derive(Debug, Default)]
struct GraphStats {
    nodes: usize,
    arcs: usize,
    patterns: u128,
    bytes: usize,
}

#[derive(Debug)]
struct Arc {
    count: usize,
    target: usize,
}

#[derive(Debug)]
struct Node {
    demand: usize,
    capacity: u32,
    arcs: Vec<Arc>,
}

#[derive(Debug)]
struct PatternDag {
    nodes: Vec<Node>,
}

impl PatternDag {
    // Expected O(V + E) construction with hashed suffix sharing. Each arc chooses
    // the count of one demand row. Nodes with the same remaining capacity and
    // next row share all suffix choices. Terminal capacities share one sink.
    fn build(
        problem: &OneDProblem,
        stock: &Stock1D,
        budget: &mut Budget,
        stats: &mut GraphStats,
    ) -> Option<Self> {
        if !budget.spend() {
            return None;
        }
        let capacity = stock.adjusted_capacity();
        let mut nodes = vec![Node { demand: 0, capacity, arcs: Vec::new() }];
        stats.nodes += 1;
        let mut index = HashMap::from([((0, capacity), 0)]);
        let mut cursor = 0;
        while cursor < nodes.len() {
            let demand_index = nodes[cursor].demand;
            if demand_index == problem.demands.len() {
                cursor += 1;
                continue;
            }
            let demand = &problem.demands[demand_index];
            let capacity = nodes[cursor].capacity;
            let weight = demand.length.saturating_add(stock.kerf);
            let max_count = demand.quantity.min((capacity / weight) as usize);
            for count in (0..=max_count).rev() {
                if !budget.spend() {
                    return None;
                }
                let next_demand = demand_index + 1;
                let next_capacity = if next_demand == problem.demands.len() {
                    0
                } else {
                    capacity - (u64::from(weight) * count as u64) as u32
                };
                let key = (next_demand, next_capacity);
                let target = if let Some(&target) = index.get(&key) {
                    target
                } else {
                    if !budget.spend() {
                        return None;
                    }
                    let target = nodes.len();
                    nodes.push(Node {
                        demand: next_demand,
                        capacity: next_capacity,
                        arcs: Vec::new(),
                    });
                    stats.nodes += 1;
                    index.insert(key, target);
                    target
                };
                nodes[cursor].arcs.push(Arc { count, target });
                stats.arcs += 1;
            }
            cursor += 1;
        }
        let mut paths = vec![0_u128; nodes.len()];
        for (index, node) in nodes.iter().enumerate().rev() {
            paths[index] = if node.demand == problem.demands.len() {
                1
            } else {
                node.arcs.iter().fold(0_u128, |sum, arc| sum.saturating_add(paths[arc.target]))
            };
        }
        // The all-zero path is not a usable bar pattern.
        stats.patterns = stats.patterns.saturating_add(paths[0].saturating_sub(1));
        stats.bytes = stats
            .bytes
            .saturating_add(nodes.capacity().saturating_mul(size_of::<Node>()))
            .saturating_add(
                nodes
                    .iter()
                    .map(|n| n.arcs.capacity().saturating_mul(size_of::<Arc>()))
                    .sum::<usize>(),
            );
        Some(Self { nodes })
    }
}

struct Paths<'a> {
    graph: &'a PatternDag,
    stack: Vec<(usize, usize)>,
    counts: Vec<usize>,
    remaining: Vec<usize>,
}

impl<'a> Paths<'a> {
    fn new(graph: &'a PatternDag, remaining: &[usize]) -> Self {
        Self {
            graph,
            stack: vec![(0, 0)],
            counts: vec![0; remaining.len()],
            remaining: remaining.to_vec(),
        }
    }

    // Iterative DFS uses O(demand types) memory regardless of represented paths.
    // Every examined arc spends budget, including paths rejected by remaining demand.
    fn next(&mut self, budget: &mut Budget) -> std::result::Result<Option<Vec<usize>>, ()> {
        while let Some((node_index, next_arc)) = self.stack.last_mut() {
            let node = &self.graph.nodes[*node_index];
            if node.demand == self.counts.len() {
                self.stack.pop();
                if self.counts.iter().any(|&count| count != 0) {
                    return Ok(Some(self.counts.clone()));
                }
                continue;
            }
            let Some(arc) = node.arcs.get(*next_arc) else {
                self.stack.pop();
                continue;
            };
            if !budget.spend() {
                return Err(());
            }
            *next_arc += 1;
            if arc.count <= self.remaining[node.demand] {
                self.counts[node.demand] = arc.count;
                self.stack.push((arc.target, 0));
            }
        }
        Ok(None)
    }
}

#[derive(Hash, PartialEq, Eq)]
struct State {
    remaining: Vec<usize>,
    usage: Vec<usize>,
}

struct Search<'a> {
    problem: &'a OneDProblem,
    graphs: &'a [PatternDag],
    incumbent: OneDSolution,
    remaining: Vec<usize>,
    usage: Vec<usize>,
    path: Vec<(usize, Vec<usize>)>,
    waste: u64,
    memo: HashMap<State, u64>,
    memo_limit: usize,
    budget: Budget,
    states: usize,
    patterns: usize,
    truncated: Option<&'static str>,
}

impl Search<'_> {
    // Exponential integer demand-state search. The memo table only merges equal
    // remaining demand AND equal stock usage; lower waste then dominates because
    // bar count, stock cost and all available continuations are identical. The
    // cache is disabled unless all cost sums are exactly representable integers;
    // it stores at most exact_pattern_limit records.
    fn visit(&mut self) -> Result<()> {
        if !self.budget.spend() {
            self.truncated = Some("search budget");
            return Ok(());
        }
        self.states += 1;
        let missing: usize = self.remaining.iter().sum();
        if (missing, self.path.len(), self.waste)
            <= (
                self.incumbent.unplaced.len(),
                self.incumbent.stock_count,
                self.incumbent.total_waste,
            )
        {
            let candidate = self.solution()?;
            if candidate.is_better_than(&self.incumbent) {
                self.incumbent = candidate;
            }
        }
        if missing == 0
            || (self.incumbent.unplaced.is_empty() && self.path.len() >= self.incumbent.stock_count)
        {
            return Ok(());
        }
        if self.path.len() >= MAX_DEPTH {
            self.truncated = Some("selected-bar depth limit");
            return Ok(());
        }
        let state = State { remaining: self.remaining.clone(), usage: self.usage.clone() };
        if self.memo.get(&state).is_some_and(|&waste| waste <= self.waste) {
            return Ok(());
        }
        if self.memo.len() < self.memo_limit {
            self.memo.insert(state, self.waste);
        }
        let graphs = self.graphs;
        for (stock_index, graph) in graphs.iter().enumerate() {
            let stock = &self.problem.stock[stock_index];
            if stock.available.is_some_and(|cap| self.usage[stock_index] >= cap) {
                continue;
            }
            let mut paths = Paths::new(graph, &self.remaining);
            loop {
                let counts = match paths.next(&mut self.budget) {
                    Ok(Some(counts)) => counts,
                    Ok(None) => break,
                    Err(()) => {
                        self.truncated = Some("search budget");
                        return Ok(());
                    }
                };
                self.patterns += 1;
                let adjusted_used: u64 = counts
                    .iter()
                    .zip(&self.problem.demands)
                    .map(|(&count, d)| {
                        count as u64 * u64::from(d.length.saturating_add(stock.kerf))
                    })
                    .sum();
                let waste = u64::from(stock.adjusted_capacity()) - adjusted_used;
                for (left, &count) in self.remaining.iter_mut().zip(&counts) {
                    *left -= count;
                }
                self.usage[stock_index] += 1;
                self.waste += waste;
                self.path.push((stock_index, counts));
                self.visit()?;
                if let Some((_, counts)) = self.path.pop() {
                    for (left, count) in self.remaining.iter_mut().zip(counts) {
                        *left += count;
                    }
                }
                self.waste -= waste;
                self.usage[stock_index] -= 1;
                if self.truncated.is_some() {
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn solution(&self) -> Result<OneDSolution> {
        let mut bins = Vec::with_capacity(self.path.len());
        for (stock_index, counts) in &self.path {
            let mut bin = PackedBin::new(*stock_index);
            for (demand_index, &count) in counts.iter().enumerate() {
                let demand = &self.problem.demands[demand_index];
                for _ in 0..count {
                    let piece = PieceInstance {
                        demand_index,
                        name: demand.name.clone(),
                        length: demand.length,
                    };
                    if !bin.add_piece(piece, &self.problem.stock[*stock_index]) {
                        return Err(BinPackingError::Unsupported(
                            "arc-flow pattern failed independent bin reconstruction".into(),
                        ));
                    }
                }
            }
            bins.push(bin);
        }
        let unplaced: Vec<_> = self
            .remaining
            .iter()
            .enumerate()
            .flat_map(|(demand_index, &count)| {
                let demand = &self.problem.demands[demand_index];
                (0..count).map(move |_| PieceInstance {
                    demand_index,
                    name: demand.name.clone(),
                    length: demand.length,
                })
            })
            .collect();
        Ok(OneDSolution::from_bins(
            "arc_flow",
            false,
            None,
            &self.problem.stock,
            &bins,
            &unplaced,
            SolverMetrics1D {
                iterations: 0,
                generated_patterns: 0,
                enumerated_patterns: 0,
                explored_states: 0,
                notes: Vec::new(),
            },
        ))
    }
}
