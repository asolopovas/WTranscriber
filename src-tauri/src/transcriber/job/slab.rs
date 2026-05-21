use crate::config::{Config, Engine};

const DEFAULT_SLAB_SEC: f64 = 60.0;
const ANDROID_WHISPER_CPP_SLAB_SEC: f64 = 15.0;
const CALIBRATION_SLAB_SEC: f64 = 10.0;
const MIN_SLAB_SEC: f64 = 0.25;
const MAX_SLAB_SEC: f64 = 600.0;

const fn default_slab_sec(config: &Config) -> f64 {
    if cfg!(target_os = "android") && matches!(config.engine, Engine::WhisperCpp) {
        ANDROID_WHISPER_CPP_SLAB_SEC
    } else {
        DEFAULT_SLAB_SEC
    }
}

fn bounded_slab_seconds(raw: &str) -> Option<f64> {
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(MIN_SLAB_SEC, MAX_SLAB_SEC))
}

pub(super) fn slab_sec(config: &Config) -> f64 {
    std::env::var("WT_SLAB_SEC")
        .ok()
        .and_then(|s| bounded_slab_seconds(&s))
        .unwrap_or_else(|| default_slab_sec(config))
}

pub(super) fn first_slab_sec(config: &Config, total_sec: f64) -> f64 {
    let normal = slab_sec(config);
    let calibration = std::env::var("WT_FIRST_SLAB_SEC")
        .ok()
        .and_then(|s| bounded_slab_seconds(&s))
        .unwrap_or(CALIBRATION_SLAB_SEC);
    if total_sec <= calibration * 1.5 {
        normal
    } else {
        calibration.min(normal)
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set_env(key: &str, value: &str) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn unset_env(key: &str) {
        unsafe {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn slab_sec_env_handling() {
        let _g = ENV_LOCK.lock().unwrap();

        let config = Config::default();
        unset_env("WT_SLAB_SEC");
        assert!((slab_sec(&config) - DEFAULT_SLAB_SEC).abs() < f64::EPSILON);

        for invalid in ["not-a-number", "inf", "NaN"] {
            set_env("WT_SLAB_SEC", invalid);
            assert!(
                (slab_sec(&config) - DEFAULT_SLAB_SEC).abs() < f64::EPSILON,
                "input {invalid:?} should fall back to default"
            );
        }

        set_env("WT_SLAB_SEC", "0");
        assert!((slab_sec(&config) - MIN_SLAB_SEC).abs() < f64::EPSILON);

        set_env("WT_SLAB_SEC", "999999");
        assert!((slab_sec(&config) - MAX_SLAB_SEC).abs() < f64::EPSILON);

        set_env("WT_SLAB_SEC", "30");
        assert!((slab_sec(&config) - 30.0).abs() < f64::EPSILON);

        unset_env("WT_SLAB_SEC");
    }

    #[test]
    fn first_slab_uses_short_calibration_for_long_audio() {
        let _g = ENV_LOCK.lock().unwrap();
        let config = Config::default();
        unset_env("WT_SLAB_SEC");
        unset_env("WT_FIRST_SLAB_SEC");
        assert!((first_slab_sec(&config, 120.0) - CALIBRATION_SLAB_SEC).abs() < f64::EPSILON);
        assert!((first_slab_sec(&config, 12.0) - DEFAULT_SLAB_SEC).abs() < f64::EPSILON);
    }
}
