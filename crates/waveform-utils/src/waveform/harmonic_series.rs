use num_complex::Complex64 as c64;
use std::{
    f64::consts::{PI, SQRT_2, TAU},
    sync::RwLock,
};

use crate::waveform::{SQRT_6, Sawtooth, Sine, Square, Triangle, auto_fn_impl};

pub trait FourierCoef {
    /// Returns the $n$-th fourier coefficient of a waveform, defined by
    /// $c_n = integral_0^1 f(t) e^(- 2 upright(pi i) n t) dif t$.
    /// Because our waveforms are real valued functions without DC component, we have $c_(-n) = overline(c_n)$ and
    /// $c_0 = 0$, where $overline(z)$ denotes the complex conjugate.
    /// The waveform should be normalized so by Parseval's identity so we have $sum_(n = -oo)^(oo) abs(c_n)^2 = 1$.
    fn coef(&self, n: isize) -> c64;
}

impl FourierCoef for Sine {
    /// $f(t) = sin(2 upright(pi) t) = (upright(e)^(2 upright(pi i) n t) - upright(e)^(-2 upright(pi i) n t)) /
    /// (2 upright(i))$
    fn coef(&self, n: isize) -> c64 {
        match n {
            1 => c64::new(0., 0.5),
            -1 => c64::new(0., -0.5),
            _ => c64::ZERO,
        }
    }
}

impl FourierCoef for Square {
    fn coef(&self, n: isize) -> c64 {
        if n % 2 == 0 {
            c64::ZERO
        } else {
            c64::new(0., SQRT_2 / PI / n as f64)
        }
    }
}

impl FourierCoef for Triangle {
    fn coef(&self, n: isize) -> c64 {
        if n % 2 == 0 {
            c64::ZERO
        } else {
            let im = 2. * SQRT_6 / (n as f64 * PI).powi(2);
            c64::new(0., if n % 4 == 1 { im } else { -im })
        }
    }
}

impl FourierCoef for Sawtooth {
    fn coef(&self, n: isize) -> c64 {
        c64::new(0., SQRT_6 / PI / n as f64 / 2.)
    }
}

/// Waveform defined by Fourier series coefficients.
/// The waveform function is $f(t) = op(Re) sum_(n = 1)^N c_n e^(- 2 upright(pi i) n t)$.
#[derive(Debug)]
pub struct HarmonicSeries {
    coefs: Vec<c64>,
    correction: RwLock<Option<c64>>,
}

impl Clone for HarmonicSeries {
    fn clone(&self) -> Self {
        Self {
            coefs: self.coefs.clone(),
            correction: RwLock::new(None),
        }
    }
}

fn calc_correction(coefs: &[c64]) -> c64 {
    let norm = coefs.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt();
    let arg = coefs[0].arg();
    c64::from_polar(norm, arg)
}

impl HarmonicSeries {
    fn correction(&self) -> c64 {
        if self.correction.read().unwrap().is_none() {
            self.correction
                .write()
                .unwrap()
                .replace(calc_correction(&self.coefs));
        }
        self.correction.read().unwrap().unwrap()
    }

    pub fn from_complex(coefs: impl IntoIterator<Item = c64>) -> Self {
        let coefs = coefs.into_iter().collect::<Vec<_>>();
        let correction = calc_correction(&coefs);
        HarmonicSeries {
            coefs,
            correction: RwLock::new(Some(correction)),
        }
    }

    pub fn from_amp_phase(data: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let mut iter = data.into_iter().peekable();
        let arg = iter.peek().unwrap().1;
        let iter = iter.map(|(r, t)| (r.powi(2), c64::from_polar(r, t * TAU)));
        let mut coefs = Vec::with_capacity(iter.size_hint().0.saturating_add(1));
        let mut norm = 0.0;
        for (r_sqr, coef) in iter {
            norm += r_sqr;
            coefs.push(coef);
        }
        norm = norm.sqrt();
        HarmonicSeries {
            coefs,
            correction: RwLock::new(Some(c64::from_polar(norm, arg))),
        }
    }

    /// Construct a harmonic series from a finite number of Fourier series terms of another waveform.
    pub fn from_other_waveform<T: FourierCoef>(waveform: &T, n_terms: usize) -> Self {
        Self::from_complex((1..=n_terms).map(|n| waveform.coef(n as isize)))
    }

    fn _coef(&self, idx: usize) -> c64 {
        self.coefs.get(idx as usize).copied().unwrap_or_default() / self.correction() / 2.
    }
}

impl FourierCoef for HarmonicSeries {
    fn coef(&self, n: isize) -> c64 {
        match n {
            0 => c64::ZERO,
            n @ 1.. => self._coef((n - 1) as usize),
            n => self._coef((-n - 1) as usize).conj(),
        }
    }
}

auto_fn_impl!(HarmonicSeries, (f64,), f64);
impl Fn<(f64,)> for HarmonicSeries {
    extern "rust-call" fn call(&self, args: (f64,)) -> Self::Output {
        (self
            .coefs
            .iter()
            .copied()
            .enumerate()
            .map(|(n, c)| c64::from_polar(1., -TAU * (n + 1) as f64 * args.0) * c)
            .sum::<c64>()
            / self.correction())
        .im
    }
}
