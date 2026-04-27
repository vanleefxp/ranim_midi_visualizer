use std::any::Any;

use derivative::Derivative;

pub trait Envelope: Any {
    /// Returns the volume level at time `t` (in seconds) starting from the moment the sound is triggered.
    /// The return value should be in the range `0.0..=1.0` where `1.0` represents the maximum volume.
    fn on_attack(&self, t: f64) -> f64;
    /// Returns the volume level at time `t` (in seconds) starting from the moment the sound is stopped.
    /// The return value should be in the range `0.0..=1.0` where `1.0` represents the maximum volume.
    fn on_release(&self, t: f64) -> f64;
    /// The duration the sound lasts after the moment it is stopped (in seconds).
    fn release_time(&self) -> f64;
}

impl<T: Fn(f64) -> f64 + 'static> Envelope for T {
    fn on_attack(&self, t: f64) -> f64 {
        self(t)
    }
    fn on_release(&self, _t: f64) -> f64 {
        0.
    }
    fn release_time(&self) -> f64 {
        0.
    }
}

/// An empty envelope. Has constant volume level `1.0` while the sound is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoEnvelope;

impl Envelope for NoEnvelope {
    fn on_attack(&self, _t: f64) -> f64 {
        1.
    }
    fn on_release(&self, _t: f64) -> f64 {
        0.
    }
    fn release_time(&self) -> f64 {
        0.
    }
}

/// An envelope with a linear fading in and out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fading {
    /// Fade in time (in seconds)
    pub fade_in: f64,
    /// Fade out time (in seconds)
    pub fade_out: f64,
}

pub fn fading(fade_in: f64, fade_out: f64) -> Fading {
    Fading { fade_in, fade_out }
}

impl Fading {
    const NONE: Self = Fading {
        fade_in: 0.,
        fade_out: 0.,
    };
}

impl Default for Fading {
    fn default() -> Self {
        Self::NONE
    }
}

impl Envelope for Fading {
    fn on_attack(&self, t: f64) -> f64 {
        let &Self { fade_in, .. } = self;
        if t < fade_in { t / fade_in } else { 1. }
    }

    fn on_release(&self, t: f64) -> f64 {
        let &Self { fade_out, .. } = self;
        if t < fade_out {
            1. - (t / fade_out)
        } else {
            0.
        }
    }

    fn release_time(&self) -> f64 {
        self.fade_out
    }
}

impl From<NoEnvelope> for Fading {
    fn from(_value: NoEnvelope) -> Self {
        Fading::NONE
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ADSR {
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
}

pub fn adsr(attack: f64, decay: f64, sustain: f64, release: f64) -> ADSR {
    ADSR {
        attack,
        decay,
        sustain,
        release,
    }
}

impl ADSR {
    const NONE: Self = ADSR {
        attack: 0.,
        decay: 0.,
        sustain: 1.,
        release: 0.,
    };
}

impl Default for ADSR {
    fn default() -> Self {
        Self::NONE
    }
}

impl Envelope for ADSR {
    fn on_attack(&self, t: f64) -> f64 {
        let &Self {
            attack,
            decay,
            sustain,
            ..
        } = self;
        if t <= attack {
            t / attack
        } else if t <= attack + decay {
            let t_rel = (t - attack) / decay;
            1. + (sustain - 1.) * t_rel
        } else {
            sustain
        }
    }

    fn on_release(&self, t: f64) -> f64 {
        let &Self {
            release, sustain, ..
        } = self;
        let t_rel = t / release;
        if t_rel < 1. {
            (1. - t_rel) * sustain
        } else {
            0.
        }
    }

    fn release_time(&self) -> f64 {
        self.release
    }
}

impl From<NoEnvelope> for ADSR {
    fn from(_value: NoEnvelope) -> Self {
        ADSR::NONE
    }
}

impl From<Fading> for ADSR {
    fn from(value: Fading) -> Self {
        let Fading { fade_in, fade_out } = value;
        ADSR {
            attack: fade_in,
            decay: 0.,
            sustain: 1.,
            release: fade_out,
        }
    }
}

/// An exponential decay envelope.
/// The only controllable parameter is the decay magnitude $lambda$.
/// The envelope is described by $f(t) = upright(e)^(- lambda t)$.
#[derive(Derivative)]
#[derivative(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExpDecay(#[derivative(Default(value = "1.0"))] pub f64);

impl Envelope for ExpDecay {
    fn on_attack(&self, t: f64) -> f64 {
        (-t * self.0).exp()
    }
    fn on_release(&self, _t: f64) -> f64 {
        0.
    }
    fn release_time(&self) -> f64 {
        0.
    }
}
