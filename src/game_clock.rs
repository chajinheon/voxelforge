//! World time that can be paused independently from real time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameClock {
    elapsed: f64,
    paused: bool,
}
impl GameClock {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, real_dt: f32) {
        if !self.paused && real_dt.is_finite() && real_dt > 0.0 {
            self.elapsed += f64::from(real_dt);
        }
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn seconds(&self) -> f32 {
        self.elapsed as f32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_stops_game_clock() {
        let mut clock = GameClock::new();
        clock.update(1.0);
        let before = clock.seconds();
        clock.set_paused(true);
        clock.update(10.0);
        assert_eq!(clock.seconds(), before);
        clock.set_paused(false);
        clock.update(0.5);
        assert_eq!(clock.seconds(), before + 0.5);
    }
}
