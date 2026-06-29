use alloc::vec::Vec;
use spectral3d::Mesh;
use crate::perlin::Fbm;
use crate::rng::Rng;

const RIM_STEEPNESS: f64 = 0.5;

/// A low-frequency directional lobe (or dent). Modulates radius by dir·lobe_dir to
/// create large-scale directional asymmetry.
pub struct Lobe {
    pub dir: [f64; 3],
    pub amp: f64,
    pub sharp: f64,
    pub sign: f64, // +1 bulge / -1 dent
}

pub struct AsteroidParams {
    pub base_radius: f64,
    pub noise_amplitude: f64,
    pub noise_frequency: f64,
    pub octaves: usize,
    pub num_craters: usize,
    pub crater_min: f64,
    pub crater_max: f64,
    pub axis_scale: [f64; 3],
    pub lobes: Vec<Lobe>,
}

impl Default for AsteroidParams {
    fn default() -> Self {
        AsteroidParams {
            base_radius: 1.0,
            noise_amplitude: 0.35,
            noise_frequency: 1.6,
            octaves: 5,
            num_craters: 8,
            crater_min: 0.15,
            crater_max: 0.45,
            axis_scale: [1.0, 1.0, 1.0],
            lobes: Vec::new(),
        }
    }
}

struct Crater {
    center: [f64; 3],
    radius: f64,
    depth: f64,
    rim_width: f64,
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    libm::sqrt(dx * dx + dy * dy + dz * dz)
}

// Polynomial smooth min/max, so crater floors and rims transition cleanly without hard edges
fn smin(a: f64, b: f64, k: f64) -> f64 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}
fn smax(a: f64, b: f64, k: f64) -> f64 {
    -smin(-a, -b, k)
}

pub fn sculpt(mut mesh: Mesh, params: &AsteroidParams, rng: &mut Rng) -> Mesh {
    let fbm = Fbm::new(rng.next_seed(), params.octaves, params.noise_frequency);

    let mut craters = Vec::with_capacity(params.num_craters);
    for _ in 0..params.num_craters {
        craters.push(Crater {
            center: rng.unit_vector(),
            radius: rng.range(params.crater_min, params.crater_max),
            depth: rng.range(0.4, 0.9),
            rim_width: rng.range(0.1, 0.3),
        });
    }

    for v in &mut mesh.vertices {
        let dir = *v; // blank vertices already sit on the unit sphere, i.e. the radial direction

        // 1) fBm overall relief
        let n = fbm.get([dir[0], dir[1], dir[2]]);
        let mut elevation = 1.0 + params.noise_amplitude * n;

        // 2) low-frequency directional lobes, large-scale asymmetry that feeds odd-order harmonics
        for lobe in &params.lobes {
            let d = dir[0] * lobe.dir[0] + dir[1] * lobe.dir[1] + dir[2] * lobe.dir[2];
            if d > 0.0 {
                // Route the power through the libm crate (pure soft-float,
                // bit-identical across targets), never std f64::powf (platform libm,
                // differs by a ULP across architectures and would flip PoScan's s).
                elevation += lobe.sign * lobe.amp * libm::pow(d, lobe.sharp);
            }
        }

        // 3) stack craters, each a cavity ringed by a slight rim bump and flattened outside
        for c in &craters {
            let x = dist(dir, c.center) / c.radius;
            let cavity = x * x - 1.0;
            let rim_x = libm::fmin(x - 1.0 - c.rim_width, 0.0);
            let rim = RIM_STEEPNESS * rim_x * rim_x;
            let mut shape = smax(cavity, -c.depth, 0.5);
            shape = smin(shape, rim, 0.3);
            elevation += shape * params.noise_amplitude;
        }

        // 4) settle along the radius, clamp to a positive minimum radius
        let r = libm::fmax(params.base_radius * elevation, 0.3 * params.base_radius);

        // 5) three-axis anisotropy
        *v = [
            dir[0] * r * params.axis_scale[0],
            dir[1] * r * params.axis_scale[1],
            dir[2] * r * params.axis_scale[2],
        ];
    }

    mesh
}
