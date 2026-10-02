//! Port of `Noise.kt`. Rust has no companion objects: the gradient tables are `static` data, read without any
//! initialisation check, so `ImprovedNoise` and `ImprovedNoiseHoisted` collapse into a single type. The
//! `noise-hoisted` workload runs the same code and exists only so the Rust column lines up with the Kotlin one.

use crate::util::JavaRandom;

static GRADIENT_X: [f64; 16] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0];
static GRADIENT_Y: [f64; 16] = [1.0, 1.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
static GRADIENT_Z: [f64; 16] = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 0.0, 1.0, 0.0, -1.0];

/// Ken Perlin's improved noise, shaped like vanilla's `ImprovedNoise`.
pub struct ImprovedNoise {
    xo: f64,
    yo: f64,
    zo: f64,
    permutation: [i32; 256],
}

impl ImprovedNoise {
    pub fn new(random: &mut JavaRandom) -> Self {
        let xo = random.next_double() * 256.0;
        let yo = random.next_double() * 256.0;
        let zo = random.next_double() * 256.0;
        let mut permutation = [0i32; 256];
        for (i, p) in permutation.iter_mut().enumerate() {
            *p = i as i32;
        }
        for i in 0..256 {
            let j = random.next_int(256 - i as i32) as usize;
            permutation.swap(i, i + j);
        }
        ImprovedNoise { xo, yo, zo, permutation }
    }

    #[inline]
    fn p(&self, i: i32) -> i32 {
        self.permutation[(i & 255) as usize]
    }

    #[inline]
    fn fade(t: f64) -> f64 {
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }

    #[inline]
    fn lerp(t: f64, a: f64, b: f64) -> f64 {
        a + t * (b - a)
    }

    #[inline]
    fn grad(hash: i32, x: f64, y: f64, z: f64) -> f64 {
        let h = (hash & 15) as usize;
        GRADIENT_X[h] * x + GRADIENT_Y[h] * y + GRADIENT_Z[h] * z
    }

    pub fn noise(&self, x: f64, y: f64, z: f64) -> f64 {
        let dx = x + self.xo;
        let dy = y + self.yo;
        let dz = z + self.zo;
        let ix = dx.floor() as i32;
        let iy = dy.floor() as i32;
        let iz = dz.floor() as i32;
        let fx = dx - ix as f64;
        let fy = dy - iy as f64;
        let fz = dz - iz as f64;

        let a = self.p(ix) + iy;
        let aa = self.p(a) + iz;
        let ab = self.p(a + 1) + iz;
        let b = self.p(ix + 1) + iy;
        let ba = self.p(b) + iz;
        let bb = self.p(b + 1) + iz;

        let u = Self::fade(fx);
        let v = Self::fade(fy);
        let w = Self::fade(fz);
        Self::lerp(
            w,
            Self::lerp(
                v,
                Self::lerp(u, Self::grad(self.p(aa), fx, fy, fz), Self::grad(self.p(ba), fx - 1.0, fy, fz)),
                Self::lerp(u, Self::grad(self.p(ab), fx, fy - 1.0, fz), Self::grad(self.p(bb), fx - 1.0, fy - 1.0, fz)),
            ),
            Self::lerp(
                v,
                Self::lerp(
                    u,
                    Self::grad(self.p(aa + 1), fx, fy, fz - 1.0),
                    Self::grad(self.p(ba + 1), fx - 1.0, fy, fz - 1.0),
                ),
                Self::lerp(
                    u,
                    Self::grad(self.p(ab + 1), fx, fy - 1.0, fz - 1.0),
                    Self::grad(self.p(bb + 1), fx - 1.0, fy - 1.0, fz - 1.0),
                ),
            ),
        )
    }
}

/// Several octaves of noise, each twice the frequency and half the amplitude of the previous one.
pub struct PerlinOctaves {
    levels: Vec<ImprovedNoise>,
}

impl PerlinOctaves {
    pub fn new(random: &mut JavaRandom, octaves: usize) -> Self {
        PerlinOctaves { levels: (0..octaves).map(|_| ImprovedNoise::new(random)).collect() }
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut frequency = 1.0;
        let mut amplitude = 1.0;
        let mut total = 0.0;
        for level in &self.levels {
            total += level.noise(x * frequency, y * frequency, z * frequency) * amplitude;
            frequency *= 2.0;
            amplitude *= 0.5;
        }
        total
    }
}
