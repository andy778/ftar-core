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

/// splitmix64: the next number from state `z`.
fn splitmix(z: &mut u64) -> u64 {
    *z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut x = *z;
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Rejections before Marsaglia and Tsang's gamma gives up (each round
/// accepts with probability above 0.95, so this never happens).
const GAMMA_TRIES: usize = 1000;

impl Rng {
    pub(crate) fn from_seed(seed: &str) -> Self {
        let mut z = fnv1a(seed);
        let s = [
            splitmix(&mut z),
            splitmix(&mut z),
            splitmix(&mut z),
            splitmix(&mut z),
        ];
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
        for _ in 0..GAMMA_TRIES {
            let x = self.normal();
            let v = (1.0 + c * x).powi(3);
            if v <= 0.0 {
                continue;
            }
            if accepts(self.uniform_open(), x, d, v) {
                return d * v;
            }
        }
        f64::NAN
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

/// Marsaglia and Tsang's acceptance test for the candidate `d·v`.
fn accepts(u: f64, x: f64, d: f64, v: f64) -> bool {
    u.ln() < 0.5 * x * x + d - d * v + d * v.ln()
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
    fn generators_match_their_reference_vectors() {
        // FNV-1a 64.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
        // splitmix64 from 0.
        let mut z = 0;
        assert_eq!(splitmix(&mut z), 0xe220_a839_7b1d_cdaf);
        assert_eq!(splitmix(&mut z), 0x6e78_9e6a_a1b9_65f4);
        // xoshiro256** from the state [1, 2, 3, 4].
        let mut r = Rng([1, 2, 3, 4]);
        let out: Vec<u64> = (0..4).map(|_| r.next_u64()).collect();
        assert_eq!(out, [11520, 0, 1_509_978_240, 1_215_971_899_390_074_240]);
    }

    /// The first samples of each distribution for one seed, so a seed gives
    /// the same numbers in every version (to 1e-12: `ln`, `exp` and `cos`
    /// may differ in the last bit between platforms).
    #[test]
    fn samples_are_pinned_per_seed() {
        use Distribution::*;
        let pins: &[(Distribution, [f64; 3])] = &[
            (
                Beta {
                    alpha: 2.0,
                    beta: 5.0,
                },
                [0.08000668704907728, 0.04192316189785182, 0.3626755936061592],
            ),
            (
                Gamma {
                    alpha: 0.5,
                    lambda: 2.0,
                },
                [
                    0.010963555447515195,
                    0.07227661986065469,
                    1.5919142732457714,
                ],
            ),
            (
                Gamma {
                    alpha: 1.0,
                    lambda: 1.0,
                },
                [0.15159523191068783, 0.05068136616992545, 3.508083328814201],
            ),
            (
                Gamma {
                    alpha: 3.0,
                    lambda: 1.0,
                },
                [0.6680706781778929, 1.3920496374502849, 0.9617540498851753],
            ),
            (
                LogNormal {
                    mu: 1.0,
                    sigma: 0.5,
                },
                [1.0993010779810637, 2.3267708530071913, 1.613939434803387],
            ),
            (
                LogUniform {
                    lower: 0.001,
                    upper: 0.1,
                },
                [
                    0.06651635469880625,
                    0.015584725130526599,
                    0.001574703914522453,
                ],
            ),
            (
                Normal {
                    mu: 1.0,
                    sigma: 2.0,
                },
                [
                    -2.6213016231639683,
                    0.37792562533225427,
                    -1.0852878230451375,
                ],
            ),
            (
                Triangular {
                    lower: 0.0,
                    upper: 4.0,
                    mode: 1.0,
                },
                [2.96925786112288, 1.7991353703354434, 0.6280109984584953],
            ),
            (
                Uniform {
                    lower: 1.0,
                    upper: 3.0,
                },
                [2.8229284405238366, 2.1926991469819157, 1.197198907092418],
            ),
        ];
        for (d, want) in pins {
            let mut r = Rng::from_seed("pin");
            for w in want {
                let got = r.sample(&Param::Dist(*d));
                assert!(close(got, *w, 1e-12), "{d:?}: {got} vs {w}");
            }
        }
    }

    #[test]
    fn triangular_inverts_its_distribution_function() {
        // lower 0, mode 1, upper 4: F(1) = 1/4.
        assert_eq!(triangular(0.04, 0.0, 4.0, 1.0), 0.4);
        assert_eq!(triangular(0.25, 0.0, 4.0, 1.0), 1.0);
        // Right of the mode: 4 - sqrt((1 - u) * 4 * 3).
        assert_eq!(triangular(0.8125, 0.0, 4.0, 1.0), 2.5);
        assert_eq!(triangular(0.5, 2.0, 2.0, 2.0), 2.0, "no width");
        assert_eq!(
            triangular(0.5, 1.0, 3.0, 3.0),
            1.0 + (0.5f64 * 2.0 * 2.0).sqrt()
        );
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
