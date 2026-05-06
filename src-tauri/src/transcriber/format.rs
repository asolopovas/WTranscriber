use std::time::Duration;

pub fn format_hms(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn output_filename(input_base: &str, model: &str) -> String {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    format!("{input_base}_{model}_{stamp}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hms_with_hours() {
        assert_eq!(format_hms(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn formats_hms_under_hour() {
        assert_eq!(format_hms(Duration::from_secs(75)), "1:15");
    }
}
