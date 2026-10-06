use crate::nodes::*;

#[test]
fn leases_lowest_free_and_recycles_on_drop() {
    let nodes = Nodes::new(2);
    let a = nodes.acquire().expect("node 1");
    assert_eq!(a.number(), 1);
    let b = nodes.acquire().expect("node 2");
    assert_eq!(b.number(), 2);
    assert!(nodes.acquire().is_none(), "pool exhausted at max");
    drop(a);
    let c = nodes.acquire().expect("node 1 again");
    assert_eq!(c.number(), 1, "freed node is reused lowest-first");
    drop(b);
    drop(c);
    assert_eq!(nodes.acquire().expect("empty pool").number(), 1);
}

#[test]
fn max_is_clamped_to_at_least_one() {
    let nodes = Nodes::new(0);
    let lease = nodes.acquire().expect("one node");
    assert_eq!(lease.number(), 1);
    assert!(nodes.acquire().is_none());
}
