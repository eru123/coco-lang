//! GC stress tests: high-volume allocation, refcount collection, and tracing
//! collection of cyclic garbage.

use std::any::Any;
use std::cell::RefCell;
use std::fmt;

use coco_gc::{GcRef, Heap};

/// A traceable test node holding child `GcRef`s in a `RefCell` so cycles can
/// be constructed by adding a back-edge after allocation. The interpreter's
/// real `Value` lives in another crate; this mirrors the shape the tracer
/// expects (a `Clone + Debug` type the closure can downcast).
#[derive(Clone, Debug)]
struct Node {
    children: RefCell<Vec<GcRef>>,
}

impl Node {
    fn new(children: Vec<GcRef>) -> Self {
        Self {
            children: RefCell::new(children),
        }
    }
    fn push_child(&self, child: GcRef) {
        self.children.borrow_mut().push(child);
    }
    fn children_vec(&self) -> Vec<GcRef> {
        self.children.borrow().clone()
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.children.borrow().len())
    }
}

impl coco_gc::GcObj for Node {
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Allocate a `Node` on the heap and return its `GcRef`.
fn alloc_node(heap: &mut Heap, children: Vec<GcRef>) -> GcRef {
    let (id, _) = heap.allocate(Node::new(children));
    id
}

/// Tracer used by every test: downcast to `Node` and clone its children.
fn node_tracer(any: &dyn Any) -> Vec<GcRef> {
    any.downcast_ref::<Node>()
        .map(|n| n.children_vec())
        .unwrap_or_default()
}

#[test]
fn refcount_collects_unreferenced_objects() {
    let mut heap = Heap::new();
    let id = alloc_node(&mut heap, vec![]);
    // Drop the only reference: dec_ref to zero.
    assert!(heap.dec_ref(id));
    heap.collect();
    // After collection the object is gone.
    assert_eq!(heap.live_count(), 0);
}

#[test]
fn allocates_100k_objects_without_leak() {
    let mut heap = Heap::new();
    let mut keep: Vec<GcRef> = Vec::new();
    for i in 0..100_000 {
        let id = alloc_node(&mut heap, vec![]);
        // Keep every 1000th alive; the rest become unreachable (refcount 0
        // after we drop the Gc handle — simulated by dec_ref).
        if i % 1000 == 0 {
            keep.push(id);
        } else {
            heap.dec_ref(id);
        }
    }
    heap.collect();
    // Only the retained objects survive.
    assert_eq!(heap.live_count(), keep.len());
    assert_eq!(heap.stats.allocations, 100_000);
}

#[test]
fn tracing_collects_unreachable_cycle() {
    let mut heap = Heap::new();
    // Build a true 2-node cycle A <-> B by adding a back-edge after alloc.
    let a = alloc_node(&mut heap, vec![]);
    let b = alloc_node(&mut heap, vec![a]);
    // Back-edge A -> B completes the cycle A <-> B. Neither is rooted.
    {
        // SAFETY: a is the only handle and we hold no concurrent borrow; we
        // mutate through the RefCell inside the Node.
        let obj_ptr = {
            let (_id, ptr) = (a, heap.obj_as_any(a).unwrap() as *const dyn Any);
            ptr as *const Node
        };
        unsafe { (*obj_ptr).push_child(b) };
    }
    // No roots: the cycle is unreachable and must be collected despite each
    // node retaining a non-zero refcount (A and B reference each other).
    let roots: Vec<GcRef> = vec![];
    heap.collect_tracing(&roots, node_tracer);
    assert_eq!(heap.live_count(), 0);
    assert!(heap.stats.traced_freed >= 2);
}

#[test]
fn tracing_keeps_rooted_cycle_alive() {
    let mut heap = Heap::new();
    // Build A <-> B and root A. Both must survive.
    let a = alloc_node(&mut heap, vec![]);
    let b = alloc_node(&mut heap, vec![a]);
    {
        let obj_ptr = heap.obj_as_any(a).unwrap() as *const dyn Any as *const Node;
        unsafe { (*obj_ptr).push_child(b) };
    }
    let roots = vec![a];
    heap.collect_tracing(&roots, node_tracer);
    // Rooted cycle (2 nodes) survives; nothing was freed.
    assert_eq!(heap.live_count(), 2);
    assert_eq!(heap.stats.traced_freed, 0);
}

#[test]
fn tracing_collects_large_list_and_nested_maps() {
    let mut heap = Heap::new();
    // A "list" node with many child refs, plus nested map-like nodes.
    let mut leaves: Vec<GcRef> = Vec::new();
    for _ in 0..1000 {
        leaves.push(alloc_node(&mut heap, vec![]));
    }
    let list = alloc_node(&mut heap, leaves.clone());
    // A map node referencing the list (rooted), and an unrooted map referencing it.
    let map_rooted = alloc_node(&mut heap, vec![list]);
    let _map_orphan = alloc_node(&mut heap, vec![list]); // orphan, unreachable

    let roots = vec![map_rooted];
    heap.collect_tracing(&roots, node_tracer);
    // Survivors: map_rooted, list, and 1000 leaves = 1002. Orphan map freed.
    assert_eq!(heap.live_count(), 1002);
    assert_eq!(heap.stats.traced_freed, 1);
}

#[test]
fn repeated_collection_cycles_are_stable() {
    let mut heap = Heap::new();
    let mut roots: Vec<GcRef> = Vec::new();
    for _ in 0..50 {
        // Each iteration: allocate a small graph, root it, collect. All
        // rooted graphs must persist across every subsequent collection.
        let b = alloc_node(&mut heap, vec![]);
        let a = alloc_node(&mut heap, vec![b]);
        roots.push(a);
        heap.collect_tracing(&roots, node_tracer);
    }
    // 50 iterations * 2 nodes each, all rooted => 100 alive after every cycle.
    assert_eq!(heap.live_count(), 100);
}
