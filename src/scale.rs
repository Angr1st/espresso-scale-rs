use core::marker::PhantomData;
use alloc::boxed::Box;
use embedded_graphics::pixelcolor::raw;

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

struct Scale<S: ScaleState> {
    state: alloc::boxed::Box<ActualScaleState>,
    marker: core::marker::PhantomData<S>
}

impl Scale<Init> {
    fn new() -> Self {
        Self { state: Box::new(ActualScaleState::default()), marker: PhantomData }
    }
}

enum Init {}
enum Weighing {}
enum Menu {}
enum Calibrating {}

trait ScaleState {}
impl ScaleState for Init {}
impl ScaleState for Weighing {}
impl ScaleState for Menu {}
impl ScaleState for Calibrating {}

#[derive(Debug)]
pub struct ActualScaleState {
    offset: i32,
    scale: f32,
    scale_reciprocal: f32,
    mode: ScaleMode,
}

impl Default for ActualScaleState {
    fn default() -> Self {
        Self {
            offset: 0,
            scale: 1.0,
            scale_reciprocal: 1.0 / 1.0,
            mode: ScaleMode::default(),
        }
    }
}

impl ActualScaleState {
    pub fn init(&mut self, offset: i32) {
        self.set_offset(offset);
        self.mode = ScaleMode::Weighing;
    }

    /// Calibrate with 100g weight
    pub fn calibrate(&mut self, raw_value: i32) {
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
