use ranim::{
    color::{AlphaColor, Srgb},
    core::components::width::Width,
    prelude::{FillColor, StrokeColor, StrokeWidth},
};

#[derive(Clone, Copy, Debug)]
pub struct StrokeAndFill {
    pub fill_rgba: AlphaColor<Srgb>,
    pub stroke_rgba: AlphaColor<Srgb>,
    pub stroke_width: Width,
}

impl StrokeColor for StrokeAndFill {
    fn stroke_color(&self) -> AlphaColor<Srgb> {
        self.stroke_rgba
    }

    fn set_stroke_opacity(&mut self, opacity: f32) -> &mut Self {
        self.stroke_rgba = self.stroke_rgba.with_alpha(opacity);
        self
    }

    fn set_stroke_color(&mut self, color: AlphaColor<Srgb>) -> &mut Self {
        self.stroke_rgba = color;
        self
    }
}

impl FillColor for StrokeAndFill {
    fn fill_color(&self) -> AlphaColor<Srgb> {
        self.fill_rgba
    }
    fn set_fill_opacity(&mut self, opacity: f32) -> &mut Self {
        self.fill_rgba = self.fill_rgba.with_alpha(opacity);
        self
    }
    fn set_fill_color(&mut self, color: AlphaColor<Srgb>) -> &mut Self {
        self.fill_rgba = color;
        self
    }
}

impl StrokeWidth for StrokeAndFill {
    fn stroke_width(&self) -> f32 {
        self.stroke_width.0
    }

    fn apply_stroke_func(&mut self, f: impl for<'a> Fn(&'a mut [Width])) -> &mut Self {
        f(std::slice::from_mut(&mut self.stroke_width));
        self
    }
}
