use num_complex::{Complex64 as c64};
use std::f64::consts::TAU;

/// Waveform defined by Fourier series coefficients.
/// The waveform function is $f(t) = op(Re) sum_(n = 1)^N c_n e^(- 2 upright(pi i) n t)$.
#[derive(Debug, Clone)]
pub struct HarmonicSeries {
    coefs: Vec<c64>,
    normalize_factor: f64,
}

impl HarmonicSeries {
    pub fn from_complex(coefs: impl IntoIterator<Item = c64>) -> Self {
        let coefs = coefs.into_iter().collect::<Vec<_>>();
        let norm = coefs.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
        HarmonicSeries { coefs, normalize_factor: 1. / norm }
    }

    pub fn from_amp_phase(data: impl IntoIterator<Item = (f64, f64)>) -> Self {
        Self::from_complex(data.into_iter().map(|(r, t)| c64::from_polar(r, t * TAU)))
    }

    fn _coef(&self, idx: usize) -> c64 {
        self.coefs.get(idx as usize).copied().unwrap_or_default() / self.normalize_factor
    }

    /// Returns the $n$th fourier coefficient of the harmonic series, defined by
    /// $c_n = integral_0^1 f(t) e^(- 2 upright(pi i) n t) dif t$.
    /// Because our waveforms are real valued functions without DC component, we have $c_(-n) = overline(c_n)$ and
    /// $c_0 = 0$.
    pub fn coef(&self, n: isize) -> c64 {
        match n {
            0 => c64::ZERO,
            n @ 1.. => self._coef((n - 1) as usize),
            n => self._coef((-n - 1) as usize).conj(),
        }
    }
}

impl Fn<(f64,)> for HarmonicSeries {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        self.coefs.iter().copied().enumerate()
        .map(|(n, c)| c64::from_polar(1., -TAU * (n + 1) as f64 * args.0) * c)
        .sum::<c64>().re / self.normalize_factor
    }
}

impl FnMut<(f64,)> for HarmonicSeries {
    extern "rust-call" fn call_mut(&mut self, args: (f64,)) -> Self::Output {
        self.call(args)
    }
}

impl FnOnce<(f64,)> for HarmonicSeries {
    type Output = f64;
    extern "rust-call" fn call_once(self, args: (f64,)) -> Self::Output {
        self.call(args)
    }
}