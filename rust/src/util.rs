//! Port of `MathUtil.kt`: the deterministic random generator, the checksum mixer and the math helpers.

/// `java.util.Random`, bit for bit, same as `JavaRandom` in Kotlin.
#[derive(Clone)]
pub struct JavaRandom {
    state: i64,
}

impl JavaRandom {
    const MULTIPLIER: i64 = 0x5DEECE66D;
    const ADDEND: i64 = 0xB;
    const MASK: i64 = (1 << 48) - 1;
    const DOUBLE_UNIT: f64 = 1.0 / (1u64 << 53) as f64;

    pub fn new(seed: i64) -> Self {
        JavaRandom { state: (seed ^ Self::MULTIPLIER) & Self::MASK }
    }

    #[inline]
    pub fn next(&mut self, bits: u32) -> i32 {
        self.state = (self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(Self::ADDEND)) & Self::MASK;
        ((self.state as u64) >> (48 - bits)) as i32
    }

    #[inline]
    pub fn next_int(&mut self, bound: i32) -> i32 {
        let mut r = self.next(31);
        let m = bound - 1;
        if bound & m == 0 {
            return ((bound as i64 * r as i64) >> 31) as i32;
        }
        let mut u = r;
        r = u % bound;
        while u.wrapping_sub(r).wrapping_add(m) < 0 {
            u = self.next(31);
            r = u % bound;
        }
        r
    }

    pub fn next_double(&mut self) -> f64 {
        let high = (self.next(26) as i64) << 27;
        let low = self.next(27) as i64;
        (high + low) as f64 * Self::DOUBLE_UNIT
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }
}

/// FNV-1a style mixing used by every checksum.
#[inline]
pub fn mix_hash(hash: i64, value: i64) -> i64 {
    (hash ^ value).wrapping_mul(0x100000001B3)
}

/// `floor(value).toInt()`: Rust's `as` saturates and maps NaN to 0, exactly like Kotlin's `toInt()`.
#[inline]
pub fn floor_int(value: f64) -> i32 {
    value.floor() as i32
}

/// Deterministic atan2 approximation, same operations in the same order as the Kotlin one.
pub fn fast_atan2(y: f64, x: f64) -> f64 {
    let ax = x.abs();
    let ay = y.abs();
    if ax == 0.0 && ay == 0.0 {
        return 0.0;
    }
    let a = if ax < ay { ax / ay } else { ay / ax };
    let s = a * a;
    let mut r = ((-0.0464964749 * s + 0.15931422) * s - 0.327622764) * s * a + a;
    if ay > ax {
        r = 1.57079637 - r;
    }
    if x < 0.0 {
        r = 3.14159274 - r;
    }
    if y < 0.0 {
        r = -r;
    }
    r
}
