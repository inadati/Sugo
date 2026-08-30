//! Pure on-demand lease liveness check for Nipper-driven runs.
//!
//! A run's lease is "live" when the most recent heartbeat is within `ttl_secs`
//! of `now`. Both timestamps are RFC3339 strings (as produced by the clock).

use chrono::DateTime;

/// Default lease TTL: heartbeat is 5s, three missed beats => 15s.
pub const LEASE_TTL_SECS: i64 = 15;

/// Returns true when `last_heartbeat_at` is present and within `ttl_secs` of `now`.
/// Returns false when the timestamp is absent or unparseable, or the lease expired.
pub fn is_live(last_heartbeat_at: Option<&str>, now: &str, ttl_secs: i64) -> bool {
    let Some(hb) = last_heartbeat_at else {
        return false;
    };
    let (Ok(hb), Ok(now)) = (
        DateTime::parse_from_rfc3339(hb),
        DateTime::parse_from_rfc3339(now),
    ) else {
        return false;
    };
    let elapsed = now.signed_duration_since(hb).num_seconds();
    (0..=ttl_secs).contains(&elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_heartbeat_is_dead() {
        assert!(!is_live(None, "2026-06-28T00:00:10+09:00", 15));
    }

    #[test]
    fn within_ttl_is_live() {
        // 14s elapsed < 15s
        assert!(is_live(
            Some("2026-06-28T00:00:00+09:00"),
            "2026-06-28T00:00:14+09:00",
            15
        ));
    }

    #[test]
    fn exactly_ttl_is_live() {
        assert!(is_live(
            Some("2026-06-28T00:00:00+09:00"),
            "2026-06-28T00:00:15+09:00",
            15
        ));
    }

    #[test]
    fn beyond_ttl_is_dead() {
        assert!(!is_live(
            Some("2026-06-28T00:00:00+09:00"),
            "2026-06-28T00:00:16+09:00",
            15
        ));
    }

    #[test]
    fn clock_skew_negative_elapsed_is_dead() {
        // now is before the heartbeat (clock rewound) => negative elapsed => dead.
        assert!(!is_live(
            Some("2026-06-28T00:00:10+09:00"),
            "2026-06-28T00:00:00+09:00",
            15
        ));
    }

    #[test]
    fn unparseable_is_dead() {
        assert!(!is_live(
            Some("not-a-date"),
            "2026-06-28T00:00:10+09:00",
            15
        ));
    }
}
