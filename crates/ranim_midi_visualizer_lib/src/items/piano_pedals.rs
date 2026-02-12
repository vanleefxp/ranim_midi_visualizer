use std::cell::{Ref, RefCell};

use ranim::{
    color::AlphaColor,
    core::{Extract, core_item::CoreItem},
    glam::DVec3,
    items::vitem::{VItem, geometry::anchor::Origin, svg::SvgItem},
    prelude::*,
};

use crate::midi::PedalType;

const PEDAL_SVG_SRC: &str = include_str!("../assets/pedals.svg");

#[derive(Debug, Clone, PartialEq)]
pub struct PianoPedals {
    origin: DVec3,
    height: f64,
    status: [u8; 3],
    items: RefCell<Option<[VItem; 3]>>,
}

impl Locate<PianoPedals> for Origin {
    fn locate(&self, target: &PianoPedals) -> DVec3 {
        target.origin
    }
}

impl PianoPedals {
    fn create_items(&self) -> [VItem; 3] {
        let i_svg = SvgItem::new(PEDAL_SVG_SRC).with(|item| {
            item.scale_to(ScaleHint::PorportionalY(self.height))
                .set_color(AlphaColor::WHITE)
                .set_fill_opacity(0.)
                .set_stroke_width(0.015)
                .set_stroke_opacity(1.);
            let top = item.aabb()[1] - item.aabb_size().x * 0.5 * DVec3::X;
            let shift = self.origin - top;
            item.shift(shift);
        });
        let i_pedals: Vec<VItem> = i_svg.into();
        i_pedals.try_into().expect("Should have exacly 3 items")
    }

    fn items<'a>(&'a self) -> Ref<'a, [VItem; 3]> {
        if self.items.borrow().is_none() {
            let items = self.create_items();
            self.items.replace(Some(items));
            self.set_item_opacity();
        }
        Ref::map(self.items.borrow(), |x| {
            x.as_ref().expect("shouldn't be none")
        })
    }

    fn set_item_opacity(&self) {
        match self.items.borrow_mut().as_mut() {
            Some(items) => {
                for (i, item) in items.iter_mut().enumerate() {
                    item.set_fill_opacity(self.status[i] as f32 / 127. * 0.5);
                }
            }
            None => {}
        }
    }

    pub fn set_pedal_status(&mut self, pedal: PedalType, status: u8) -> &mut Self {
        self.status[pedal as usize] = status;
        self.set_item_opacity();
        self
    }

    pub fn pedal_status(&self) -> [u8; 3] {
        self.status
    }

    pub fn set_height(&mut self, height: f64) -> &mut Self {
        self.height = height;
        if let Some(items) = self.items.borrow_mut().as_mut() {
            items.scale_to_at(ScaleHint::PorportionalY(height), AabbPoint(DVec3::Y));
        }
        self
    }

    pub fn height(&self) -> f64 {
        self.height
    }
}

impl Default for PianoPedals {
    fn default() -> Self {
        Self {
            origin: DVec3::ZERO,
            height: 0.75,
            status: [0; 3],
            items: Default::default(),
        }
    }
}

impl Aabb for PianoPedals {
    fn aabb(&self) -> [DVec3; 2] {
        self.items().aabb()
    }
}

impl Shift for PianoPedals {
    fn shift(&mut self, offset: DVec3) -> &mut Self {
        self.origin += offset;
        if let Some(items) = self.items.borrow_mut().as_mut() {
            items.shift(offset);
        }
        self
    }
}

impl Extract for PianoPedals {
    type Target = CoreItem;

    fn extract_into(&self, buf: &mut Vec<Self::Target>) {
        self.items().extract_into(buf);
    }
}
