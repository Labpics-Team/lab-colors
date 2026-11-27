use super::*;

fn peers(input_order: &[usize], reverse_vertices: bool) -> PeerSearchGraph {
    let count = input_order.len();
    let vertex = |index| {
        if reverse_vertices {
            2 * count - 1 - index
        } else {
            index
        }
    };
    let mut colors = vec![VertexColorV1::new(0); 2 * count];
    let mut edges = Vec::new();
    for input in 0..count {
        colors[vertex(count + input)]
            .push_bytes(&[1])
            .expect("static target colour fits");
        edges.push(EdgeV1 {
            from: vertex(count + input),
            to: vertex(input),
            role: 7,
        });
    }
    PeerSearchGraph {
        colors,
        edges,
        targets: input_order
            .iter()
            .map(|&input| (input, vertex(count + input)))
            .collect(),
        inputs: input_order
            .iter()
            .map(|&input| (input, vertex(input)))
            .collect(),
        constraints: Vec::new(),
    }
}

fn observed(
    peer: &PeerSearchGraph,
    cases: &[usize],
    matrix: &[[[u8; 3]; 2]],
) -> ColoredGraph<VertexColorV1> {
    peer.observed_graph(cases, |case, input| Ok(matrix[case][input]))
        .expect("finite RGB matrix is valid")
        .expect("observations distinguish the two ports")
}

fn identity(graph: &ColoredGraph<VertexColorV1>) -> Vec<u8> {
    canonical_form(graph, b"observed-peer-test\0")
        .expect("small finite graph has a canonical form")
        .preimage
}

#[test]
fn observed_order_preserves_joint_rows_under_all_row_and_port_permutations() {
    let matrix = [
        [[0; 3], [0; 3]],
        [[17, 45, 91], [110, 82, 64]],
        [[110, 82, 64], [17, 45, 91]],
        [[255; 3], [255; 3]],
    ];
    let expected = identity(&observed(&peers(&[0, 1], false), &[0, 1, 2, 3], &matrix));
    for input_order in [[0, 1], [1, 0]] {
        for reverse_vertices in [false, true] {
            let peer = peers(&input_order, reverse_vertices);
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        for d in 0..4 {
                            let cases = [a, b, c, d];
                            if (0..4).any(|i| cases[..i].contains(&cases[i])) {
                                continue;
                            }
                            assert_eq!(identity(&observed(&peer, &cases, &matrix)), expected);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn observed_order_distinguishes_joint_correlations_with_equal_column_multisets() {
    let first = [
        [[0; 3], [0; 3]],
        [[64; 3], [128; 3]],
        [[128; 3], [64; 3]],
        [[255; 3], [255; 3]],
    ];
    let second = [
        [[0; 3], [64; 3]],
        [[64; 3], [0; 3]],
        [[128; 3], [128; 3]],
        [[255; 3], [255; 3]],
    ];
    for input in 0..2 {
        let mut a = first.map(|row| row[input]);
        let mut b = second.map(|row| row[input]);
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
    }
    let peer = peers(&[0, 1], false);
    let a = observed(&peer, &[0, 1, 2, 3], &first);
    let b = observed(&peer, &[0, 1, 2, 3], &second);
    assert_ne!(identity(&a), identity(&b));
}

#[test]
fn observed_order_retains_every_rgb_channel_byte() {
    let peer = peers(&[0, 1], false);
    for channel in 0..3 {
        let mut identities = std::collections::HashSet::new();
        for byte in 0..=255 {
            let mut value = [37, 83, 149];
            value[channel] = byte;
            let matrix = [
                [[0; 3], [0; 3]],
                [value, [211, 191, 173]],
                [[255; 3], [255; 3]],
            ];
            assert!(identities.insert(identity(&observed(&peer, &[0, 1, 2], &matrix))));
        }
    }
    let red = [
        [[0; 3], [0; 3]],
        [[1, 0, 0], [0, 0, 0]],
        [[255; 3], [255; 3]],
    ];
    let green = [
        [[0; 3], [0; 3]],
        [[0, 1, 0], [0, 0, 0]],
        [[255; 3], [255; 3]],
    ];
    assert_ne!(
        identity(&observed(&peer, &[0, 1, 2], &red)),
        identity(&observed(&peer, &[0, 1, 2], &green))
    );
}

#[test]
fn observed_order_allocates_channels_per_port_and_rows_per_case() {
    let count = 8;
    let cases: Vec<_> = (0..64).collect();
    let peer = peers(&(0..count).collect::<Vec<_>>(), false);
    let graph = peer
        .observed_graph(&cases, |case, input| Ok([((case + input) % 64) as u8; 3]))
        .expect("complete RGB matrix")
        .expect("ports have different observations");
    assert_eq!(graph.vertex_count(), 2 * count + 3 * count + cases.len());
    assert_eq!(graph.edge_count(), count + 3 * count * (cases.len() + 1));
}

#[test]
fn observed_order_keeps_identical_ports_and_rejects_missing_observations() {
    let peer = peers(&[0, 1], false);
    assert!(
        peer.observed_graph(&[0, 1], |case, _| Ok([case as u8; 3]))
            .expect("valid observations")
            .is_none()
    );
    assert!(peer.observed_graph(&[0, 1], |_, _| Err(())).is_err());
}
