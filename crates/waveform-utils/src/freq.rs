pub trait ToFrequency {
    fn to_frequency(&self) -> f64;
}

impl ToFrequency for i8 {
    fn to_frequency(&self) -> f64 {
        440. * 2.0f64.powf(*self as f64 / 12. - 0.75)
    }
}