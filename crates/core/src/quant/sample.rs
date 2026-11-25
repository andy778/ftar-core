//! Pseudo-random numbers and the distributions of the text format.
//!
//! The generator is xoshiro256** seeded through splitmix64, so a run is
//! reproducible per seed on every platform. PFTA uses Python's Mersenne
//! Twister, so the samples themselves differ from PFTA's; their
//! distributions are the same.

use crate::text::{Distribution, Param};

pub(crate) struct Rng([u64; 4]);

/// 64-bit FNV-1a, to turn a seed text into a number.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Rng {
    pub(crate) fn from_seed(seed: &str) -> Self {
        let mut z = fnv1a(seed);
        let mut next = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        let s = [next(), next(), next(), next()];
        // xoshiro's state must not be all zero; splitmix64 never gives four zeros.
        Rng(s)
    }

    fn next_u64(&mut self) -> u64 {
        let s = &mut self.0;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)`.
    pub(crate) fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `(0, 1]`, safe to take the logarithm of.
    fn uniform_open(&mut self) -> f64 {
        1.0 - self.uniform()
    }

    fn normal(&mut self) -> f64 {
        // Box–Muller.
        let r = (-2.0 * self.uniform_open().ln()).sqrt();
        r * (std::f64::consts::TAU * self.uniform()).cos()
    }

    /// Gamma with shape `alpha` and scale 1 (Marsaglia and Tsang).
    fn gamma(&mut self, alpha: f64) -> f64 {
        if alpha < 1.0 {
            let u = self.uniform_open();
            return self.gamma(alpha + 1.0) * u.powf(1.0 / alpha);
        }
        let d = alpha - 1.0 / 3.0;
        let c = 1.0 / (9.0 * d).sqrt();
        loop {
            let x = self.normal();
            let v = (1.0 + c * x).powi(3);
            if v <= 0.0 {
                continue;
            }
            let u = self.uniform_open();
            if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
                return d * v;
            }
        }
    }

    pub(crate) fn sample(&mut self, p: &Param) -> f64 {
        let Param::Dist(d) = p else {
            let Param::Value(x) = p else { return f64::NAN };
            return *x;
        };
        match *d {
            Distribution::Beta { alpha, beta } => {
                let x = self.gamma(alpha);
                let y = self.gamma(beta);
                x / (x + y)
            }
            Distribution::Gamma { alpha, lambda } => self.gamma(alpha) / lambda,
            Distribution::LogNormal { mu, sigma } => (mu + sigma * self.normal()).exp(),
            Distribution::LogUniform { lower, upper } => {
                (lower.ln() + (upper.ln() - lower.ln()) * self.uniform()).exp()
            }
            Distribution::Normal { mu, sigma } => mu + sigma * self.normal(),
            Distribution::Triangular { lower, upper, mode } => {
                triangular(self.uniform(), lower, upper, mode)
            }
            Distribution::Uniform { lower, upper } => lower + (upper - lower) * self.uniform(),
        }
    }
}

/// Inverse distribution function of the triangular distribution.
fn triangular(u: f64, low: f64, high: f64, mode: f64) -> f64 {
    let width = high - low;
    if width == 0.0 {
        return low;
    }
    let c = (mode - low) / width;
    if u < c {
        low + (u * width * (mode - low)).sqrt()
    } else {
        high - ((1.0 - u) * width * (high - mode)).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_var(p: Param, n: usize) -> (f64, f64) {
        let mut r = Rng::from_seed("test");
        let xs: Vec<f64> = (0..n).map(|_| r.sample(&p)).collect();
        let m = xs.iter().sum::<f64>() / n as f64;
        let v = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n as f64;
        (m, v)
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1e-12)
    }

    #[test]
    fn distributions_have_their_moments() {
        use Distribution::*;
        let n = 200_000;
        let cases: &[(Distribution, f64, f64)] = &[
            (
                Uniform {
                    lower: 1.0,
                    upper: 3.0,
                },
                2.0,
                4.0 / 12.0,
            ),
            (
                Normal {
                    mu: 5.0,
                    sigma: 2.0,
                },
                5.0,
                4.0,
            ),
            (
                Gamma {
                    alpha: 3.0,
                    lambda: 2.0,
                },
                1.5,
                0.75,
            ),
            (
                Gamma {
                    alpha: 0.5,
                    lambda: 1.0,
                },
                0.5,
                0.5,
            ),
            (
                Beta {
                    alpha: 2.0,
                    beta: 5.0,
                },
                2.0 / 7.0,
                10.0 / (49.0 * 8.0),
            ),
            (
                Triangular {
                    lower: 0.0,
                    upper: 3.0,
                    mode: 1.0,
                },
                4.0 / 3.0,
                (9.0 + 1.0 - 3.0) / 18.0,
            ),
            (
                LogNormal {
                    mu: 0.0,
                    sigma: 0.5,
                },
                (0.125f64).exp(),
                (0.25f64.exp() - 1.0) * 0.25f64.exp(),
            ),
        ];
        for (d, m, v) in cases {
            let (gm, gv) = mean_var(Param::Dist(*d), n);
            assert!(close(gm, *m, 0.01), "{d:?}: mean {gm} vs {m}");
            assert!(close(gv, *v, 0.03), "{d:?}: variance {gv} vs {v}");
        }
        // loguniform: ln X is uniform.
        let mut r = Rng::from_seed("lu");
        let d = Param::Dist(LogUniform {
            lower: 0.001,
            upper: 0.1,
        });
        let logs: Vec<f64> = (0..n).map(|_| r.sample(&d).log10()).collect();
        let m = logs.iter().sum::<f64>() / n as f64;
        assert!(close(m, -2.0, 0.01), "{m}");
        assert!(logs.iter().all(|x| (-3.0..=-1.0).contains(x)));
    }

    #[test]
    fn same_seed_same_numbers() {
        let p = Param::Dist(Distribution::Normal {
            mu: 0.0,
            sigma: 1.0,
        });
        let a: Vec<f64> = {
            let mut r = Rng::from_seed("x");
            (0..5).map(|_| r.sample(&p)).collect()
        };
        let mut r = Rng::from_seed("x");
        let b: Vec<f64> = (0..5).map(|_| r.sample(&p)).collect();
        assert_eq!(a, b);
        let mut r = Rng::from_seed("y");
        assert_ne!(a[0], r.sample(&p));
        assert_eq!(Rng::from_seed("z").sample(&Param::Value(0.25)), 0.25);
    }
}
