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

#[derive(Debug)]
pub struct Scale {
    offset: i32,
    scale: f32,
    mode: ScaleMode,
}

impl Default for Scale {
    fn default() -> Self {
        Self {
            offset: 0,
            scale: 1.0,
            mode: ScaleMode::default(),
        }
    }
}

impl Scale {
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
        let scale: f32 = (100.33 - self.offset) as f32 / raw_value as f32;
        self.set_scale(scale)
    }

    fn set_offset(&mut self, offset: i32) {
        self.offset = offset;
    }

    fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    pub fn get_offset(&self) -> i32 {
        self.offset
    }

    pub fn get_value(&self, raw_value: i32) -> i32 {
        raw_value - self.offset
    }

    pub fn get_units(&self, raw_value: i32) -> i32 {
        use micromath::F32Ext;

        let result = raw_value as f32 * self.scale;

        (result.round() as i32) + self.offset
    }

    pub fn tare(&mut self, raw_value: i32) {
        self.set_offset(raw_value)
    }
}
