//! Weighted network measures for the passing graph: PageRank, betweenness and communities.
//!
//! `weights[i][j]` is the strength of the link from `i` to `j` (passes); the diagonal is
//! ignored.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

const DAMPING: f64 = 0.85;
const PAGERANK_ITERATIONS: usize = 200;
const PAGERANK_TOLERANCE: f64 = 1e-12;

/// Weighted PageRank: a player ranks high when strong passers pass to them a lot.
///
/// Players who pass to no one spread their rank evenly. Scores add up to 1.
#[must_use]
pub fn pagerank(weights: &[Vec<f64>]) -> Vec<f64> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    let out: Vec<f64> = (0..n).map(|i| (0..n).filter(|&j| j != i).map(|j| weights[i][j]).sum()).collect();
    let mut rank = vec![1.0 / n as f64; n];
    for _ in 0..PAGERANK_ITERATIONS {
        let dangling: f64 = (0..n).filter(|&i| out[i] <= 0.0).map(|i| rank[i]).sum();
        let next: Vec<f64> = (0..n)
            .map(|j| {
                let incoming: f64 = (0..n)
                    .filter(|&i| i != j && out[i] > 0.0)
                    .map(|i| rank[i] * weights[i][j] / out[i])
                    .sum();
                (1.0 - DAMPING) / n as f64 + DAMPING * (incoming + dangling / n as f64)
            })
            .collect();
        let change: f64 = next.iter().zip(&rank).map(|(a, b)| (a - b).abs()).sum();
        rank = next;
        if change < PAGERANK_TOLERANCE {
            break;
        }
    }
    rank
}

#[derive(Clone, Copy, PartialEq)]
struct Visit {
    distance: f64,
    node: usize,
}

impl Eq for Visit {}

impl Ord for Visit {
    fn cmp(&self, other: &Self) -> Ordering {
        other.distance.total_cmp(&self.distance).then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for Visit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Weighted betweenness (Brandes): the share of shortest passing routes between other
/// players that run through each player.
///
/// A link's length is 1 / its passes; scores are scaled to 0–1 by the number of ordered
/// pairs of other players.
#[must_use]
pub fn betweenness(weights: &[Vec<f64>]) -> Vec<f64> {
    let n = weights.len();
    let mut centrality = vec![0.0; n];
    for source in 0..n {
        let mut stack = Vec::with_capacity(n);
        let mut predecessors: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut paths = vec![0.0_f64; n];
        let mut distance = vec![f64::INFINITY; n];
        paths[source] = 1.0;
        distance[source] = 0.0;
        let mut queue = BinaryHeap::new();
        queue.push(Visit { distance: 0.0, node: source });
        let mut settled = vec![false; n];
        while let Some(Visit { distance: d, node: v }) = queue.pop() {
            if settled[v] {
                continue;
            }
            settled[v] = true;
            stack.push(v);
            for w in (0..n).filter(|&w| w != v && weights[v][w] > 0.0) {
                let candidate = d + 1.0 / weights[v][w];
                let tolerance = 1e-12 * candidate.max(1.0);
                if candidate < distance[w] - tolerance {
                    distance[w] = candidate;
                    paths[w] = paths[v];
                    predecessors[w] = vec![v];
                    queue.push(Visit { distance: candidate, node: w });
                } else if (candidate - distance[w]).abs() <= tolerance {
                    paths[w] += paths[v];
                    predecessors[w].push(v);
                }
            }
        }
        let mut dependency = vec![0.0; n];
        while let Some(w) = stack.pop() {
            for &v in &predecessors[w] {
                dependency[v] += paths[v] / paths[w] * (1.0 + dependency[w]);
            }
            if w != source {
                centrality[w] += dependency[w];
            }
        }
    }
    let pairs = ((n.saturating_sub(1)) * (n.saturating_sub(2))) as f64;
    if pairs > 0.0 {
        for c in &mut centrality {
            *c /= pairs;
        }
    }
    centrality
}

/// Links in both directions added together, for community detection.
fn undirected(weights: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = weights.len();
    (0..n)
        .map(|i| (0..n).map(|j| if i == j { 0.0 } else { weights[i][j] + weights[j][i] }).collect())
        .collect()
}

/// Newman's modularity of a split of an undirected weighted graph.
#[must_use]
pub fn modularity(symmetric: &[Vec<f64>], community: &[usize]) -> f64 {
    let total: f64 = symmetric.iter().flatten().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let strength: Vec<f64> = symmetric.iter().map(|row| row.iter().sum()).collect();
    let mut q = 0.0;
    for i in 0..symmetric.len() {
        for j in 0..symmetric.len() {
            if community[i] == community[j] {
                q += symmetric[i][j] - strength[i] * strength[j] / total;
            }
        }
    }
    q / total
}

/// One Louvain pass: moves single nodes to the neighbouring community that most raises
/// modularity until nothing moves; returns whether anything moved.
fn local_moves(symmetric: &[Vec<f64>], community: &mut [usize]) -> bool {
    let n = symmetric.len();
    let total: f64 = symmetric.iter().flatten().sum();
    let strength: Vec<f64> = symmetric.iter().map(|row| row.iter().sum()).collect();
    let mut moved_any = false;
    loop {
        let mut moved = false;
        for i in 0..n {
            let current = community[i];
            let mut community_strength = vec![0.0; n];
            let mut links = vec![0.0; n];
            for j in 0..n {
                if j != i {
                    community_strength[community[j]] += strength[j];
                    links[community[j]] += symmetric[i][j];
                }
            }
            let gain = |c: usize| links[c] - strength[i] * community_strength[c] / total;
            let mut best = (current, gain(current));
            for (c, &link) in links.iter().enumerate() {
                let g = gain(c);
                if link > 0.0 && g > best.1 + 1e-12 {
                    best = (c, g);
                }
            }
            if best.0 != current {
                community[i] = best.0;
                moved = true;
                moved_any = true;
            }
        }
        if !moved {
            return moved_any;
        }
    }
}

/// Renumbers communities 0, 1, 2, … in order of first appearance.
fn relabel(community: &mut [usize]) {
    let mut seen: Vec<usize> = Vec::new();
    for c in community.iter_mut() {
        let index = seen.iter().position(|s| s == c).unwrap_or_else(|| {
            seen.push(*c);
            seen.len() - 1
        });
        *c = index;
    }
}

/// Louvain community detection on the pass counts in both directions.
///
/// Groups are players who pass among themselves more than their passing volume predicts.
/// Returns each player's group (0, 1, …) and the split's modularity.
#[must_use]
pub fn communities(weights: &[Vec<f64>]) -> (Vec<usize>, f64) {
    let symmetric = undirected(weights);
    let n = symmetric.len();
    let mut membership: Vec<usize> = (0..n).collect();
    if symmetric.iter().flatten().sum::<f64>() <= 0.0 {
        return (membership, 0.0);
    }
    let mut graph = symmetric.clone();
    loop {
        let size = graph.len();
        let mut level: Vec<usize> = (0..size).collect();
        if !local_moves(&graph, &mut level) {
            break;
        }
        relabel(&mut level);
        for m in &mut membership {
            *m = level[*m];
        }
        let groups = level.iter().max().map_or(0, |m| m + 1);
        let mut next = vec![vec![0.0; groups]; groups];
        for i in 0..size {
            for j in 0..size {
                next[level[i]][level[j]] += graph[i][j];
            }
        }
        if groups == size {
            break;
        }
        graph = next;
    }
    relabel(&mut membership);
    let q = modularity(&symmetric, &membership);
    (membership, q)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two tight triangles joined by one weak link between nodes 2 and 3.
    fn two_triangles() -> Vec<Vec<f64>> {
        let mut w = vec![vec![0.0; 6]; 6];
        for (a, b) in [(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5)] {
            w[a][b] = 10.0;
            w[b][a] = 10.0;
        }
        w[2][3] = 1.0;
        w[3][2] = 1.0;
        w
    }

    #[test]
    fn louvain_finds_the_two_triangles() {
        let (groups, q) = communities(&two_triangles());
        assert_eq!(groups[0], groups[1]);
        assert_eq!(groups[1], groups[2]);
        assert_eq!(groups[3], groups[4]);
        assert_ne!(groups[0], groups[3]);
        assert!(q > 0.4, "{q}");
    }

    #[test]
    fn the_bridge_players_carry_the_most_routes() {
        let b = betweenness(&two_triangles());
        let top = (0..6).max_by(|&x, &y| b[x].total_cmp(&b[y])).unwrap();
        assert!(top == 2 || top == 3, "{b:?}");
        assert!(b[0].abs() < 1e-12);
    }

    #[test]
    fn a_star_centre_has_betweenness_one() {
        let mut w = vec![vec![0.0; 4]; 4];
        for cell in w[0].iter_mut().skip(1) {
            *cell = 1.0;
        }
        for row in w.iter_mut().skip(1) {
            row[0] = 1.0;
        }
        let b = betweenness(&w);
        assert!((b[0] - 1.0).abs() < 1e-12, "{b:?}");
        assert!(b[1].abs() < 1e-12);
    }

    #[test]
    fn pagerank_sums_to_one_and_favours_the_player_everyone_feeds() {
        let mut w = vec![vec![0.0; 4]; 4];
        for row in w.iter_mut().skip(1) {
            row[0] = 5.0;
        }
        w[0][1] = 1.0;
        let rank = pagerank(&w);
        assert!((rank.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(rank[0] > rank[1] && rank[0] > rank[2]);
    }
}
