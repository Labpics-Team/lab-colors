#[path = "plan_after.rs"]
mod graph_after;
#[path = "plan_before.rs"]
mod graph_before;
use std::{hint::black_box, time::Instant};
const N: usize = 65536;
const BLOCK: usize = 8192;
fn fixture() -> (Vec<u32>, Vec<(u32, u32)>) {
    let ids = (0..N as u32).collect::<Vec<_>>();
    let mut edges = Vec::new();
    for i in 0..N {
        for d in [257, 17, 1] {
            if i % BLOCK >= d {
                edges.push(((i - d) as u32, i as u32));
            }
        }
    }
    (ids, edges)
}
fn graph_compile(new: bool) {
    let (ids, edges) = fixture();
    let before = graph_before::CompiledDependencyPlanV1::compile(&ids, &edges).unwrap();
    let after = graph_after::CompiledDependencyPlanV1::compile(&ids, &edges).unwrap();
    for i in 0..N {
        let b = graph_before::NodeIndexV1::new(i as u32);
        let a = graph_after::NodeIndexV1::new(i as u32);
        assert!(
            before
                .dependencies_of(b)
                .unwrap()
                .iter()
                .map(|n| n.raw())
                .eq(after.dependencies_of(a).unwrap().iter().map(|n| n.raw()))
        );
        assert!(
            before
                .dependents_of(b)
                .unwrap()
                .iter()
                .map(|n| n.raw())
                .eq(after.dependents_of(a).unwrap().iter().map(|n| n.raw()))
        );
    }
    let t = Instant::now();
    let mut check = 0;
    if new {
        for _ in 0..8 {
            let p =
                graph_after::CompiledDependencyPlanV1::compile(black_box(&ids), black_box(&edges))
                    .unwrap();
            check += black_box(p).node_count();
        }
    } else {
        for _ in 0..8 {
            let p =
                graph_before::CompiledDependencyPlanV1::compile(black_box(&ids), black_box(&edges))
                    .unwrap();
            check += black_box(p).node_count();
        }
    }
    println!("{} {check} {}", t.elapsed().as_nanos(), edges.len());
}
fn graph_run(new: bool, mode: &str) {
    let (ids, edges) = fixture();
    let p1 = graph_before::CompiledDependencyPlanV1::compile(&ids, &edges).unwrap();
    let p2 = graph_after::CompiledDependencyPlanV1::compile(&ids, &edges).unwrap();
    let mut visited = vec![false; N];
    let mut s1 = Vec::new();
    let mut s2 = Vec::new();
    let roots = |q: usize| -> [u32; 3] {
        std::array::from_fn(|j| {
            let i = match mode {
                "sparse" => (q * 17 + j * 11) % 24,
                "mixed" => {
                    if q % 2 == 0 {
                        (q * 17 + j * 11) % 24
                    } else {
                        (q * 7919 + j * 1237) % BLOCK
                    }
                }
                _ => (q * 7919 + j * 1237) % BLOCK,
            };
            (((q + j * 3) % 8) * BLOCK + i) as u32
        })
    };
    for q in 0..256 {
        let r = roots(q);
        let a = r.map(graph_before::NodeIndexV1::new);
        let b = r.map(graph_after::NodeIndexV1::new);
        let x = p1
            .affected_nodes_with_scratch(&a, &mut visited, &mut s1)
            .unwrap();
        let y = p2
            .affected_nodes_with_scratch(&b, &mut visited, &mut s2)
            .unwrap();
        assert!(x.iter().map(|n| n.raw()).eq(y.iter().map(|n| n.raw())));
    }
    let t = Instant::now();
    let mut sum = 0u64;
    if new {
        for q in 0..256 {
            let r = roots(q).map(graph_after::NodeIndexV1::new);
            let v = p2
                .affected_nodes_with_scratch(black_box(&r), &mut visited, &mut s2)
                .unwrap();
            sum += v.len() as u64;
            for (i, n) in v.iter().enumerate() {
                sum += (n.raw() as u64 + 1) * (i as u64 + 1);
            }
        }
    } else {
        for q in 0..256 {
            let r = roots(q).map(graph_before::NodeIndexV1::new);
            let v = p1
                .affected_nodes_with_scratch(black_box(&r), &mut visited, &mut s1)
                .unwrap();
            sum += v.len() as u64;
            for (i, n) in v.iter().enumerate() {
                sum += (n.raw() as u64 + 1) * (i as u64 + 1);
            }
        }
    }
    println!("{} {sum}", t.elapsed().as_nanos());
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let new = args[2] == "new";
    match args[1].as_str() {
        "graph_compile" => graph_compile(new),
        "graph_dense" => graph_run(new, "dense"),
        "graph_sparse" => graph_run(new, "sparse"),
        "graph_mixed" => graph_run(new, "mixed"),
        _ => panic!(),
    }
}
