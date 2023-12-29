use alloc::boxed::Box;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::text::{Text, Alignment};
use embedded_graphics::prelude::Point;
use embedded_graphics::Drawable;
use core::marker::PhantomData;

#[derive(Debug)]
pub enum ScaleMode {
    Init,
    Weighing,
    Menu,
    Calibrate,
}

impl Default for ScaleMode {
    fn default() -> Self {
        Self::Init
    }
}

pub struct Scale<S: ScaleState> {
    state: alloc::boxed::Box<ActualScaleState>,
    marker: core::marker::PhantomData<S>,
}

impl Scale<Init> {
    pub fn new() -> Self {
        Self {
            state: Box::new(ActualScaleState::default()),
            marker: PhantomData,
        }
    }

    pub fn init(mut self, offset: i32) -> Scale<Calibrating> {
        self.state.init(offset);
        Scale::<Calibrating>::new(self)
    }
}

impl Scale<Calibrating> {
    fn new(init: Scale<Init>) -> Self {
        Self {
            state: init.state,
            marker: PhantomData,
        }
    }

    pub fn calibrate(mut self, raw_value: i32) -> Scale<Weighing> {
        self.state.calibrate(raw_value);
        Scale::<Weighing>::new(self)
    }

    pub fn calibrate_fixed_value(mut self, scale: f32) -> Scale<Weighing> {
        self.state.set_scale(scale);
        Scale::<Weighing>::new(self)
    }
}

impl Scale<Weighing> {
    fn new(calibrating: Scale<Calibrating>) -> Self {
        Self {
            state: calibrating.state,
            marker: PhantomData,
        }
    }

    pub fn get_scale(&self) -> f32 {
        self.state.scale
    }

    pub fn get_scale_reciprocal(&self) -> f32 {
        self.state.scale_reciprocal
    }

    pub fn get_offset(&self) -> i32 {
        self.state.offset
    }

    pub fn get_value(&self, raw_value: i32) -> i32 {
        raw_value - self.state.offset
    }

    pub fn get_units(&self, raw_value: i32) -> f32 {
        use micromath::F32Ext;

        let value = raw_value - self.state.offset;

        let result = value as f32 * self.state.scale_reciprocal;

        result
    }

    pub fn tare(&mut self, raw_value: i32) {
        self.state.set_offset(raw_value)
    }
}

impl Scale<Menu> {
    pub fn new(weighing: Scale<Weighing>) -> Self {
        Self {
            state: weighing.state,
            marker: PhantomData,
        }
    }

    pub fn draw_menu<'t, Drawable>(display: &mut Drawable, text_style: MonoTextStyle<'t, Rgb565>)
        where Drawable: DrawTarget<Color = Rgb565, Error = core::convert::Infallible>
    {
        Text::with_alignment(
            "Menu",
            display.bounding_box().center() + Point::new(0, 0),
            text_style,
            Alignment::Center,
        )
        .draw(display)
        .unwrap();
    }
}

impl From<Scale<Menu>> for Scale<Weighing> {
    fn from(value: Scale<Menu>) -> Self {
        Self {
            state: value.state,
            marker: PhantomData
        }
    }
}

pub enum Init {}
pub enum Weighing {}
pub enum Menu {}
pub enum Calibrating {}
pub enum Brewing {}

pub trait ScaleState {}
impl ScaleState for Init {}
impl ScaleState for Weighing {}
impl ScaleState for Menu {}
impl ScaleState for Calibrating {}
impl ScaleState for Brewing {}

#[derive(Debug)]
pub struct ActualScaleState {
    offset: i32,
    scale: f32,
    scale_reciprocal: f32,
    mode: ScaleMode
}

impl Default for ActualScaleState {
    fn default() -> Self {
        Self {
            offset: 0,
            scale: 1.0,
            scale_reciprocal: 1.0 / 1.0,
            mode: ScaleMode::default()
        }
    }
}

impl ActualScaleState {
    fn init(&mut self, offset: i32) {
        self.set_offset(offset);
        self.mode = ScaleMode::Weighing;
    }

    /// Calibrate with 100g weight
    fn calibrate(&mut self, raw_value: i32) {
        //m = (y - b)/x
        // m scale (f32)
        // y weight in g (i32)
        // b offset (i32)
        // x raw value (i32)
        let value = raw_value - self.get_offset();
        let value = value as f32;
        let scale = value / 100.33;
        //let scale: f32 = (100.33 - self.offset as f32) / raw_value as f32;
        self.set_scale(scale)
    }

    fn set_offset(&mut self, offset: i32) {
        self.offset = offset;
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.scale_reciprocal = 1.0 / self.scale;
    }

    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    pub fn get_scale_reciprocal(&self) -> f32 {
        self.scale_reciprocal
    }

    pub fn get_offset(&self) -> i32 {
        self.offset
    }

    pub fn get_value(&self, raw_value: i32) -> i32 {
        raw_value - self.offset
    }

    pub fn get_units(&self, raw_value: i32) -> f32 {
        use micromath::F32Ext;

        let value = raw_value - self.offset;

        let result = value as f32 * self.scale_reciprocal;

        result
    }

    pub fn tare(&mut self, raw_value: i32) {
        self.set_offset(raw_value)
    }
}
