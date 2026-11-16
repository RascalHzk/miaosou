//! slab 索引集合运算（filter_nodes / intersect / difference）。

use crate::SlabIndex;
use hashbrown::HashSet;
use search_cancel::CancellationToken;

pub(super) fn filter_nodes(
    nodes: Vec<SlabIndex>,
    token: CancellationToken,
    mut predicate: impl FnMut(SlabIndex) -> bool,
) -> Option<Vec<SlabIndex>> {
    let mut filtered = Vec::with_capacity(nodes.len());
    let mut counter = 0usize;
    for index in nodes {
        // While filtering dc: dm:, lstat is slow. Thus we check cancellation more frequently.
        token.is_cancelled_sparse(counter)?;
        counter = counter.wrapping_add(4);
        if predicate(index) {
            filtered.push(index);
        }
    }
    Some(filtered)
}

pub(super) fn intersect_in_place(
    values: &mut Vec<SlabIndex>,
    rhs: &[SlabIndex],
    token: CancellationToken,
) -> Option<()> {
    token.is_cancelled()?;
    if values.is_empty() {
        return Some(());
    }
    if rhs.is_empty() {
        values.clear();
        return Some(());
    }
    let rhs_set: HashSet<SlabIndex> = rhs.iter().copied().collect();
    values.retain(|index| rhs_set.contains(index));
    Some(())
}

pub(super) fn difference_in_place(
    values: &mut Vec<SlabIndex>,
    rhs: &[SlabIndex],
    token: CancellationToken,
) -> Option<()> {
    token.is_cancelled()?;
    if values.is_empty() || rhs.is_empty() {
        return Some(());
    }
    let rhs_set: HashSet<SlabIndex> = rhs.iter().copied().collect();
    values.retain(|index| !rhs_set.contains(index));
    Some(())
}
