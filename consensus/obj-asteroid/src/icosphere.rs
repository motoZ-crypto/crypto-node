//! Icosphere blank. Subdivide an icosahedron level by level, projecting each new
//! vertex back onto the unit sphere. Gives the sculpt step a closed blank with no
//! pole bias and evenly sized triangles. Edge midpoints share a cache during
//! subdivision, so the mesh comes out welded with no cracks. That weld keeps the
//! body watertight, which spectral3d's volume integrals require.

use alloc::collections::BTreeMap;
use alloc::{vec, vec::Vec};

use spectral3d::Mesh;

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = libm::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    [v[0] / len, v[1] / len, v[2] / len]
}

pub fn icosphere(subdivisions: u32) -> Mesh {
    // The golden ratio gives the icosahedron's 12 vertices
    let t = (1.0 + libm::sqrt(5.0)) / 2.0;
    let mut vertices: Vec<[f64; 3]> = vec![
        [-1.0, t, 0.0], [1.0, t, 0.0], [-1.0, -t, 0.0], [1.0, -t, 0.0],
        [0.0, -1.0, t], [0.0, 1.0, t], [0.0, -1.0, -t], [0.0, 1.0, -t],
        [t, 0.0, -1.0], [t, 0.0, 1.0], [-t, 0.0, -1.0], [-t, 0.0, 1.0],
    ];
    for v in &mut vertices {
        *v = normalize(*v);
    }

    // 20 triangle faces, winding uniformly outward (orientable, normals point out)
    let mut faces: Vec<[u32; 3]> = vec![
        [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
        [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
        [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
        [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];

    for _ in 0..subdivisions {
        let mut midpoint: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut new_faces = Vec::with_capacity(faces.len() * 4);

        // Midpoint of an edge, reused on a cache hit so adjacent triangles share it
        let mut get_mid = |a: u32, b: u32, verts: &mut Vec<[f64; 3]>| -> u32 {
            let key = if a < b { (a, b) } else { (b, a) };
            if let Some(&idx) = midpoint.get(&key) {
                return idx;
            }
            let va = verts[a as usize];
            let vb = verts[b as usize];
            let mid = normalize([
                (va[0] + vb[0]) * 0.5,
                (va[1] + vb[1]) * 0.5,
                (va[2] + vb[2]) * 0.5,
            ]);
            let idx = verts.len() as u32;
            verts.push(mid);
            midpoint.insert(key, idx);
            idx
        };

        for f in &faces {
            let a = get_mid(f[0], f[1], &mut vertices);
            let b = get_mid(f[1], f[2], &mut vertices);
            let c = get_mid(f[2], f[0], &mut vertices);
            new_faces.push([f[0], a, c]);
            new_faces.push([f[1], b, a]);
            new_faces.push([f[2], c, b]);
            new_faces.push([a, b, c]);
        }
        faces = new_faces;
    }

    Mesh { vertices, faces }
}
