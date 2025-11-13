use std::marker::PhantomData;

use crate::breaks::BreaksMetric;
use crate::tree::{Leaf, Metric, NodeInfo};

#[derive(Clone, Copy)]
pub(crate) struct BaseUnitsIdentity<M> {
    _marker: PhantomData<M>,
}

impl<M> Default for BaseUnitsIdentity<M> {
    #[inline]
    fn default() -> Self {
        Self { _marker: PhantomData }
    }
}

impl<N, L, M> Metric<N, L> for BaseUnitsIdentity<M>
where
    N: NodeInfo<L>,
    L: Leaf,
    M: Metric<N, L>,
{
    #[inline]
    fn measure(_: &N, len: usize) -> usize {
        len
    }

    #[inline]
    fn to_base_units(_: &L, in_measured_units: usize) -> usize {
        in_measured_units
    }

    #[inline]
    fn from_base_units(_: &L, in_base_units: usize) -> usize {
        in_base_units
    }

    #[inline]
    fn is_boundary(leaf: &L, offset: usize) -> bool {
        M::is_boundary(leaf, offset)
    }

    #[inline]
    fn prev(leaf: &L, offset: usize) -> Option<usize> {
        M::prev(leaf, offset)
    }

    #[inline]
    fn next(leaf: &L, offset: usize) -> Option<usize> {
        M::next(leaf, offset)
    }

    #[inline]
    fn can_fragment() -> bool {
        M::can_fragment()
    }
}

pub(crate) type BreaksBaseMetric = BaseUnitsIdentity<BreaksMetric>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rope::{BaseMetric, RopeInfo};
    use crate::tree::{Metric, NodeInfo};

    #[test]
    fn delegates_to_inner_metric() {
        let leaf = String::from("hi\n");
        let info = <RopeInfo as NodeInfo<String>>::compute_info(&leaf);
        type Identity = BaseUnitsIdentity<BaseMetric>;

        assert_eq!(<Identity as Metric<_, _>>::measure(&info, leaf.len()), leaf.len());
        assert_eq!(<Identity as Metric<_, _>>::to_base_units(&leaf, 2), 2);
        assert_eq!(<Identity as Metric<_, _>>::from_base_units(&leaf, 1), 1);
        assert!(<Identity as Metric<_, _>>::is_boundary(&leaf, 0));
        assert_eq!(<Identity as Metric<_, _>>::prev(&leaf, leaf.len()), Some(leaf.len() - 1));
        assert_eq!(<Identity as Metric<_, _>>::next(&leaf, 0), Some(1));
        assert!(!<Identity as Metric<_, _>>::can_fragment());

        let _metric: BreaksBaseMetric = Default::default();
    }
}
