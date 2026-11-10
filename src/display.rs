//! Connect the upstream SSD1331 framebuffer to embedded-graphics 0.8 without
//! forking the driver or adding a second framebuffer.

use core::convert::Infallible;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_hal_compat::eh0_2::{blocking::spi::Write, digital::v2::OutputPin};
use ssd1331::Ssd1331;

pub struct Graphics<'a, SPI, DC>(pub &'a mut Ssd1331<SPI, DC>);

impl<SPI: Write<u8>, DC: OutputPin> OriginDimensions for Graphics<'_, SPI, DC> {
    fn size(&self) -> Size {
        let (width, height) = self.0.dimensions();
        Size::new(width.into(), height.into())
    }
}

impl<SPI: Write<u8>, DC: OutputPin> DrawTarget for Graphics<'_, SPI, DC> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        let bounds = self.bounding_box();
        for Pixel(point, color) in pixels {
            if bounds.contains(point) {
                self.0
                    .set_pixel(point.x as u32, point.y as u32, color.into_storage());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use ssd1331::{test_helpers::Pin, DisplayRotation};
    use std::vec::Vec;

    #[derive(Default)]
    struct Spi(Vec<u8>);

    impl Write<u8> for Spi {
        type Error = Infallible;

        fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
            self.0.extend_from_slice(bytes);
            Ok(())
        }
    }

    #[test]
    fn graphics_flushes_rgb565_pixels_and_clips_at_rotated_bounds() {
        for (rotation, width, height) in [
            (DisplayRotation::Rotate0, 96, 64),
            (DisplayRotation::Rotate90, 64, 96),
            (DisplayRotation::Rotate180, 96, 64),
            (DisplayRotation::Rotate270, 64, 96),
        ] {
            let mut display = Ssd1331::new(Spi::default(), Pin, rotation);
            let mut graphics = Graphics(&mut display);
            assert_eq!(graphics.size(), Size::new(width as u32, height as u32));
            graphics
                .draw_iter([
                    Pixel(Point::new(3, 2), Rgb565::new(17, 42, 9)),
                    Pixel(Point::new(width - 1, height - 1), Rgb565::BLUE),
                    Pixel(Point::new(-1, 0), Rgb565::WHITE),
                    Pixel(Point::new(0, -1), Rgb565::WHITE),
                    Pixel(Point::new(width, 0), Rgb565::WHITE),
                    Pixel(Point::new(0, height), Rgb565::WHITE),
                ])
                .unwrap();
            display.flush().unwrap();
            display.clear();
            display.flush().unwrap();

            let (spi, _) = display.release();
            // Each flush sends column/row bounds, then 96 × 64 RGB565 pixels.
            let frame_len = 96 * 64 * 2;
            assert_eq!(spi.0.len(), 2 * (6 + frame_len));
            let frame = &spi.0[6..6 + frame_len];
            let mut expected = [0; 96 * 64 * 2];
            let index = ((2 * width + 3) * 2) as usize;
            expected[index..index + 2].copy_from_slice(&[0x8d, 0x49]);
            expected[frame_len - 2..].copy_from_slice(&[0x00, 0x1f]);
            assert_eq!(frame, expected);
            assert!(spi.0[12 + frame_len..].iter().all(|byte| *byte == 0));
        }
    }
}
