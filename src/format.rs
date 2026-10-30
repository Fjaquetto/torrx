use std::time::Duration;

const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
const MAX_ETA_SECONDS: u64 = 99 * 3600 + 59 * 60 + 59;

pub fn bytes(value: u64) -> String {
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

pub fn speed(bytes_per_second: u64) -> String {
    format!("{}/s", bytes(bytes_per_second))
}

pub fn percent(fraction: f64) -> String {
    let fraction = clamp_fraction(fraction);
    if fraction >= 1.0 {
        "100%".to_owned()
    } else {
        format!("{:.1}%", fraction * 100.0)
    }
}

pub fn eta(remaining: Option<Duration>) -> String {
    let Some(remaining) = remaining else {
        return "--".to_owned();
    };
    let seconds = remaining.as_secs();
    if seconds > MAX_ETA_SECONDS {
        return ">99h".to_owned();
    }
    let (hours, minutes, secs) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m{secs:02}s")
    } else {
        format!("{secs}s")
    }
}

pub fn uptime(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (hours, minutes, secs) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    format!("{hours:02}:{minutes:02}:{secs:02}")
}

pub fn ratio(uploaded: u64, downloaded: u64) -> String {
    if downloaded == 0 {
        return "0.00".to_owned();
    }
    format!("{:.2}", uploaded as f64 / downloaded as f64)
}

pub fn truncate(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

pub fn progress_bar(fraction: f64, width: usize) -> String {
    let width = width.max(1);
    let filled = (clamp_fraction(fraction) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn clamp_fraction(fraction: f64) -> f64 {
    if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn formats_bytes_with_binary_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1), "1 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.00 KiB");
        assert_eq!(bytes(1536), "1.50 KiB");
        assert_eq!(bytes(1024 * 1024), "1.00 MiB");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3.00 GiB");
        assert_eq!(bytes(2 * 1024_u64.pow(4)), "2.00 TiB");
    }

    #[test]
    fn formats_speed() {
        assert_eq!(speed(0), "0 B/s");
        assert_eq!(speed(512), "512 B/s");
        assert_eq!(speed(1024), "1.00 KiB/s");
    }

    #[test]
    fn formats_percent_with_clamping() {
        assert_eq!(percent(0.0), "0.0%");
        assert_eq!(percent(0.421), "42.1%");
        assert_eq!(percent(1.0), "100%");
        assert_eq!(percent(1.5), "100%");
        assert_eq!(percent(-0.5), "0.0%");
        assert_eq!(percent(f64::NAN), "0.0%");
    }

    #[test]
    fn formats_eta() {
        assert_eq!(eta(None), "--");
        assert_eq!(eta(Some(Duration::from_secs(0))), "0s");
        assert_eq!(eta(Some(Duration::from_secs(42))), "42s");
        assert_eq!(eta(Some(Duration::from_secs(90))), "1m30s");
        assert_eq!(eta(Some(Duration::from_secs(3600))), "1h00m");
        assert_eq!(eta(Some(Duration::from_secs(3660))), "1h01m");
        assert_eq!(eta(Some(Duration::from_secs(MAX_ETA_SECONDS))), "99h59m");
        assert_eq!(eta(Some(Duration::from_secs(MAX_ETA_SECONDS + 1))), ">99h");
    }

    #[test]
    fn formats_uptime() {
        assert_eq!(uptime(Duration::from_secs(0)), "00:00:00");
        assert_eq!(uptime(Duration::from_secs(3661)), "01:01:01");
        assert_eq!(uptime(Duration::from_secs(90000)), "25:00:00");
    }

    #[test]
    fn formats_ratio() {
        assert_eq!(ratio(0, 0), "0.00");
        assert_eq!(ratio(100, 0), "0.00");
        assert_eq!(ratio(500, 1000), "0.50");
        assert_eq!(ratio(2000, 1000), "2.00");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly10!", 10), "exactly10!");
        assert_eq!(truncate("truncate me", 8), "truncat…");
        assert_eq!(truncate("áéíóú àèìòù", 6), "áéíóú…");
        assert_eq!(truncate("abc", 0), "");
        assert_eq!(truncate("abc", 1), "…");
    }

    #[test]
    fn renders_progress_bar() {
        assert_eq!(progress_bar(0.0, 4), "░░░░");
        assert_eq!(progress_bar(0.5, 4), "██░░");
        assert_eq!(progress_bar(1.0, 4), "████");
        assert_eq!(progress_bar(2.0, 4), "████");
        assert_eq!(progress_bar(f64::INFINITY, 4), "░░░░");
        assert_eq!(progress_bar(0.5, 0), "█");
    }
}
