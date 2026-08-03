//! The object-valued timestamp behind `date`-like properties: the Rust
//! analog of a `Temporal.ZonedDateTime` (an instant paired with an IANA
//! time zone).

use std::fmt;

use chrono::{DateTime, NaiveDate, SecondsFormat};
use chrono_tz::Tz;

/// An immutable zoned timestamp.
///
/// Equality is (instant, time zone id), stricter than chrono's
/// instant-only comparison, so a same-instant write in a different zone
/// still counts as a change (the catalog observes the same through
/// reference inequality), while true no-op writes stay suppressed.
#[derive(Debug, Clone)]
pub struct Zoned {
    datetime: DateTime<Tz>,
}

impl Zoned {
    pub fn new(datetime: DateTime<Tz>) -> Self {
        Zoned { datetime }
    }

    pub fn datetime(&self) -> &DateTime<Tz> {
        &self.datetime
    }

    /// The IANA identifier, e.g. `Europe/Berlin`.
    pub fn timezone_id(&self) -> &'static str {
        self.datetime.timezone().name()
    }

    /// The calendar date in the value's own time zone.
    pub fn date_naive(&self) -> NaiveDate {
        self.datetime.date_naive()
    }

    /// Temporal-style ISO rendering:
    /// `2026-07-07T00:00:00+02:00[Europe/Berlin]`.
    pub fn iso(&self) -> String {
        format!(
            "{}[{}]",
            self.datetime.to_rfc3339_opts(SecondsFormat::Secs, false),
            self.timezone_id()
        )
    }
}

impl PartialEq for Zoned {
    fn eq(&self, other: &Self) -> bool {
        self.datetime == other.datetime && self.timezone_id() == other.timezone_id()
    }
}

impl fmt::Display for Zoned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.iso())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(tz: Tz) -> Zoned {
        Zoned::new(tz.with_ymd_and_hms(2026, 7, 7, 0, 0, 0).unwrap())
    }

    #[test]
    fn equality_compares_instant_and_zone() {
        let berlin = at(chrono_tz::Europe::Berlin);
        assert_eq!(berlin, at(chrono_tz::Europe::Berlin));

        // The same instant expressed in another zone is a different value.
        let utc = Zoned::new(berlin.datetime().with_timezone(&chrono_tz::UTC));
        assert_eq!(berlin.datetime(), utc.datetime());
        assert_ne!(berlin, utc);
    }

    #[test]
    fn iso_renders_temporal_style() {
        assert_eq!(
            at(chrono_tz::Europe::Berlin).iso(),
            "2026-07-07T00:00:00+02:00[Europe/Berlin]"
        );
        assert_eq!(at(chrono_tz::UTC).iso(), "2026-07-07T00:00:00+00:00[UTC]");
    }

    #[test]
    fn date_naive_uses_the_own_zone() {
        let late = Zoned::new(
            chrono_tz::Europe::Berlin
                .with_ymd_and_hms(2026, 7, 7, 23, 30, 0)
                .unwrap(),
        );
        assert_eq!(
            late.date_naive(),
            NaiveDate::from_ymd_opt(2026, 7, 7).unwrap()
        );
        // In UTC that instant is still the 7th, 21:30.
        let utc = Zoned::new(late.datetime().with_timezone(&chrono_tz::UTC));
        assert_eq!(
            utc.date_naive(),
            NaiveDate::from_ymd_opt(2026, 7, 7).unwrap()
        );
    }
}
