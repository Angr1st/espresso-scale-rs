
pub enum ScaleMode {
    Init,
    Weighing,
    Menu
}

impl Default for ScaleMode {
    fn default() -> Self {
        Self::Init
    }
}

pub struct Scale {
    pub offset: i32,
    pub scale: f32
}

impl Default for Scale {
    fn default() -> Self {
        Self { offset: 0, scale: 1.0 }
    }
}

impl Scale {
    pub fn set_offset(&mut self, offset: i32) {
        self.offset = offset;
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn get_value(&self, raw_value: i32) -> i32 {
        raw_value - self.offset
    }

    pub fn get_units(&self, value: i32) -> i32 {
        use micromath::F32Ext;

        let result = value as f32 / self.scale;

        result.round() as i32
    }

    pub fn tare(&mut self, raw_value: i32) {
        self.set_offset(raw_value)
    }
}