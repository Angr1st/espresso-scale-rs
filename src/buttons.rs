use embedded_hal::digital::InputPin;

pub const POLL_INTERVAL_MS: u64 = 10;
const HOLD_TICKS: u8 = (200 / POLL_INTERVAL_MS) as u8;

/// The existing two-button tare gesture, sampled at 100 Hz independently of
/// ADC readiness and display transfers. Reports one event per combined hold.
pub struct TareButtons<PIN> {
    menu: PIN,
    tare: PIN,
    menu_ticks: u8,
    tare_ticks: u8,
    held: bool,
}

impl<PIN: InputPin> TareButtons<PIN> {
    pub fn new(menu: PIN, tare: PIN) -> Self {
        Self {
            menu,
            tare,
            menu_ticks: 0,
            tare_ticks: 0,
            held: false,
        }
    }

    /// Call once per timer tick, not once per measurement or per missed tick.
    /// Both active-low buttons need 20 consecutive pressed samples (a nominal
    /// 200 ms hold, with 10 ms resolution). Release of either rearms the gesture.
    pub fn poll(&mut self) -> bool {
        for (pin, ticks) in [
            (&mut self.menu, &mut self.menu_ticks),
            (&mut self.tare, &mut self.tare_ticks),
        ] {
            *ticks = if pin.is_low().unwrap_or(false) {
                ticks.saturating_add(1).min(HOLD_TICKS)
            } else {
                0
            };
        }
        let held = self.menu_ticks == HOLD_TICKS && self.tare_ticks == HOLD_TICKS;
        let pressed = held && !self.held;
        self.held = held;
        pressed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{cell::Cell, convert::Infallible};

    struct Pin<'a>(&'a Cell<bool>);

    impl embedded_hal::digital::ErrorType for Pin<'_> {
        type Error = Infallible;
    }

    impl InputPin for Pin<'_> {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(!self.0.get())
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(self.0.get())
        }
    }

    #[test]
    fn tare_requires_200ms_on_both_buttons_and_rearms_after_release() {
        let menu = Cell::new(true);
        let tare = Cell::new(false);
        let mut buttons = TareButtons::new(Pin(&menu), Pin(&tare));

        // Menu alone, even for longer than two seconds, must not tare.
        for _ in 0..70_000 {
            assert!(!buttons.poll());
        }
        // A short press/bounce on the second button must not tare either.
        tare.set(true);
        for _ in 0..19 {
            assert!(!buttons.poll());
        }
        tare.set(false);
        assert!(!buttons.poll());

        tare.set(true);
        for _ in 0..19 {
            assert!(!buttons.poll());
        }
        assert!(buttons.poll()); // 200 ms, including a long-held menu button.
        for _ in 0..70_000 {
            assert!(!buttons.poll()); // Never repeatedly tare a held chord.
        }

        menu.set(false);
        assert!(!buttons.poll());
        menu.set(true);
        for _ in 0..19 {
            assert!(!buttons.poll());
        }
        assert!(buttons.poll());
    }

    #[test]
    fn simultaneous_hold_is_not_lost_between_measurements() {
        let pressed = Cell::new(true);
        let mut buttons = TareButtons::new(Pin(&pressed), Pin(&pressed));
        // No ADC/display work is needed to detect the gesture at tick 20.
        let events = (1..=250).filter(|_| buttons.poll()).count();
        assert_eq!(events, 1);
    }
}
