use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSignals {
    pub fullscreen: bool,
    pub game_mode: bool,
    pub power_saver: bool,
    pub battery_percent: Option<u8>,
    pub low_battery_threshold: u8,
}

impl PlaybackSignals {
    #[must_use]
    pub fn pause_reasons(self) -> Vec<PauseReason> {
        let mut reasons = Vec::with_capacity(4);
        if self.fullscreen {
            reasons.push(PauseReason::Fullscreen);
        }
        if self.game_mode {
            reasons.push(PauseReason::GameMode);
        }
        if self.power_saver {
            reasons.push(PauseReason::PowerSaver);
        }
        if self
            .battery_percent
            .is_some_and(|percent| percent <= self.low_battery_threshold)
        {
            reasons.push(PauseReason::LowBattery);
        }
        reasons
    }

    #[must_use]
    pub fn should_pause(self) -> bool {
        !self.pause_reasons().is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PauseReason {
    Fullscreen,
    GameMode,
    PowerSaver,
    LowBattery,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_for_each_active_resource_signal() {
        let signals = PlaybackSignals {
            fullscreen: true,
            game_mode: true,
            power_saver: false,
            battery_percent: Some(14),
            low_battery_threshold: 15,
        };
        assert_eq!(
            signals.pause_reasons(),
            vec![
                PauseReason::Fullscreen,
                PauseReason::GameMode,
                PauseReason::LowBattery
            ]
        );
    }

    #[test]
    fn keeps_playing_without_active_signals() {
        let signals = PlaybackSignals {
            fullscreen: false,
            game_mode: false,
            power_saver: false,
            battery_percent: None,
            low_battery_threshold: 15,
        };
        assert!(!signals.should_pause());
    }
}
