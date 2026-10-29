//! Spending limits, in euros.
//!
//! An assistant that can read a whole project and loop on its own can spend a surprising amount in an
//! afternoon, and the bill arrives long after the afternoon is over. These are the brakes: a limit per
//! conversation and a limit per day, week and month, any of which can be off.
//!
//! Prices are in dollars and the limits are in euros, so everything is held in dollars (what the price
//! list says and what the ledger records) and converted at one editable rate when compared. The rate is
//! the user's to keep current; fetching one would make a limit move on its own.
//!
//! This module is only the arithmetic and the wording. Where the spending is recorded and when the check
//! runs belong to the host.

/// The limits, in euros. `0` (or less) switches a limit off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Per conversation.
    pub session_eur: f64,
    pub day_eur: f64,
    pub week_eur: f64,
    pub month_eur: f64,
    /// Euros per dollar.
    pub eur_per_usd: f64,
    /// The share of a limit, in percent, from which a meter turns to a warning.
    pub warn_percent: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { session_eur: 0.0, day_eur: 0.0, week_eur: 0.0, month_eur: 0.0, eur_per_usd: 0.92, warn_percent: 80.0 }
    }
}

/// What has been spent, in dollars, in each window.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spent {
    pub session_usd: f64,
    pub day_usd: f64,
    pub week_usd: f64,
    pub month_usd: f64,
}

impl Spent {
    /// The same amounts plus `usd` more in every window: a run in progress counts against all of them.
    pub fn plus(self, usd: f64) -> Spent {
        Spent {
            session_usd: self.session_usd + usd,
            day_usd: self.day_usd + usd,
            week_usd: self.week_usd + usd,
            month_usd: self.month_usd + usd,
        }
    }

    /// The same amounts plus a run in progress (`own_usd`, counted in every window) and the runs of *other*
    /// conversations still in flight (`others_usd`): those are not in this conversation's own figure but they
    /// are money the day, week and month have already committed.
    pub fn plus_run(self, own_usd: f64, others_usd: f64) -> Spent {
        Spent {
            session_usd: self.session_usd + own_usd,
            day_usd: self.day_usd + own_usd + others_usd,
            week_usd: self.week_usd + own_usd + others_usd,
            month_usd: self.month_usd + own_usd + others_usd,
        }
    }
}

/// What the runs that have not finished yet have spent so far, in dollars, by run.
///
/// The ledger only learns of a run's cost when it ends. Two conversations running at once each read the
/// same ledger, each add only their own spending to it, and together overshoot a day, week or month limit
/// neither reached alone. Every run reports its running figure here as it goes, so each sees the others'.
/// A run takes its figure out when it ends, after its line is in the ledger, so the money is never in
/// neither place.
#[derive(Default)]
pub struct InFlight {
    runs: std::sync::Mutex<std::collections::HashMap<String, f64>>,
}

impl InFlight {
    fn lock(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, f64>> {
        self.runs.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Record `run`'s spending so far; returns what all the *other* runs in flight have spent.
    pub fn report(&self, run: &str, usd: f64) -> f64 {
        let mut runs = self.lock();
        runs.insert(run.to_string(), usd.max(0.0));
        runs.iter().filter(|(id, _)| id.as_str() != run).map(|(_, v)| v).sum()
    }

    /// The run ended (or failed to start).
    pub fn finish(&self, run: &str) {
        self.lock().remove(run);
    }

    /// Everything in flight: what a run about to start must count before it is allowed to begin.
    pub fn total(&self) -> f64 {
        self.lock().values().sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    Session,
    Day,
    Week,
    Month,
}

impl Period {
    pub const ALL: [Period; 4] = [Period::Session, Period::Day, Period::Week, Period::Month];

    pub fn word(self) -> &'static str {
        match self {
            Period::Session => "session",
            Period::Day => "day",
            Period::Week => "week",
            Period::Month => "month",
        }
    }

    fn adjective(self) -> &'static str {
        match self {
            Period::Session => "Conversation",
            Period::Day => "Daily",
            Period::Week => "Weekly",
            Period::Month => "Monthly",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Reached,
}

/// One window's spending against its limit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    pub period: Period,
    pub spent_eur: f64,
    /// `None` when the limit is off.
    pub limit_eur: Option<f64>,
}

impl Meter {
    /// Spending as a share of the limit; 0 when there is no limit.
    pub fn fraction(&self) -> f64 {
        match self.limit_eur {
            Some(limit) if limit > 0.0 => self.spent_eur / limit,
            _ => 0.0,
        }
    }

    pub fn level(&self, warn_percent: f64) -> Level {
        match self.limit_eur {
            None => Level::Ok,
            Some(limit) if self.spent_eur >= limit => Level::Reached,
            Some(_) if self.fraction() * 100.0 >= warn_percent => Level::Warn,
            Some(_) => Level::Ok,
        }
    }

    /// `Daily limit reached: €1.20 of €1.00`, or the same with "is at 85%" for a warning.
    pub fn describe(&self, level: Level) -> String {
        let limit = self.limit_eur.unwrap_or(0.0);
        let what = self.period.adjective();
        match level {
            Level::Reached => format!("{what} limit reached: {} of {}", eur(self.spent_eur), eur(limit)),
            Level::Warn => format!("{what} spending is at {:.0}% of its limit: {} of {}", self.fraction() * 100.0, eur(self.spent_eur), eur(limit)),
            Level::Ok => format!("{what}: {} of {}", eur(self.spent_eur), eur(limit)),
        }
    }
}

/// `€1.20`, with a third decimal under a cent so a tiny amount does not read as nothing.
pub fn eur(amount: f64) -> String {
    if amount > 0.0 && amount < 0.01 { format!("€{amount:.3}") } else { format!("€{amount:.2}") }
}

impl Limits {
    fn rate(&self) -> f64 {
        // A missing or nonsense rate must not switch the limits off by multiplying to zero. One is a
        // little high (a dollar is worth less than a euro here), so it errs on the side of stopping.
        if self.eur_per_usd.is_finite() && self.eur_per_usd > 0.0 { self.eur_per_usd } else { 1.0 }
    }

    fn limit(&self, period: Period) -> Option<f64> {
        let v = match period {
            Period::Session => self.session_eur,
            Period::Day => self.day_eur,
            Period::Week => self.week_eur,
            Period::Month => self.month_eur,
        };
        (v.is_finite() && v > 0.0).then_some(v)
    }

    fn spent(&self, spent: &Spent, period: Period) -> f64 {
        let usd = match period {
            Period::Session => spent.session_usd,
            Period::Day => spent.day_usd,
            Period::Week => spent.week_usd,
            Period::Month => spent.month_usd,
        };
        usd * self.rate()
    }

    /// Every window's meter, in the order session, day, week, month.
    pub fn meters(&self, spent: &Spent) -> Vec<Meter> {
        Period::ALL
            .iter()
            .map(|&period| Meter { period, spent_eur: self.spent(spent, period), limit_eur: self.limit(period) })
            .collect()
    }

    /// The first limit that has been reached, if any.
    pub fn breach(&self, spent: &Spent) -> Option<(Meter, String)> {
        self.meters(spent)
            .into_iter()
            .find(|m| m.level(self.warn_percent) == Level::Reached)
            .map(|m| {
                let message = m.describe(Level::Reached);
                (m, message)
            })
    }

    /// The most pressing warning (a limit past `warn_percent` but not yet reached), if any.
    pub fn warning(&self, spent: &Spent) -> Option<(Meter, String)> {
        self.meters(spent)
            .into_iter()
            .filter(|m| m.level(self.warn_percent) == Level::Warn)
            .max_by(|a, b| a.fraction().total_cmp(&b.fraction()))
            .map(|m| {
                let message = m.describe(Level::Warn);
                (m, message)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> Limits {
        Limits { session_eur: 1.0, day_eur: 5.0, week_eur: 0.0, month_eur: 20.0, eur_per_usd: 0.5, warn_percent: 80.0 }
    }

    #[test]
    fn dollars_are_converted_at_the_rate_before_comparing() {
        // $1.60 at 0.5 is €0.80: under the €1 session limit.
        let spent = Spent { session_usd: 1.6, ..Default::default() };
        assert!(limits().breach(&spent).is_none());
        let spent = Spent { session_usd: 2.0, ..Default::default() };
        assert_eq!(limits().breach(&spent).unwrap().0.period, Period::Session);
    }

    #[test]
    fn a_limit_of_zero_is_off() {
        let spent = Spent { week_usd: 1_000_000.0, ..Default::default() };
        assert!(limits().breach(&spent).is_none(), "the weekly limit is off");
        assert_eq!(limits().meters(&spent)[2].limit_eur, None);
    }

    #[test]
    fn reaching_a_limit_exactly_stops_it() {
        let spent = Spent { day_usd: 10.0, ..Default::default() };
        let (m, text) = limits().breach(&spent).unwrap();
        assert_eq!(m.period, Period::Day);
        assert_eq!(text, "Daily limit reached: €5.00 of €5.00");
    }

    #[test]
    fn a_warning_names_the_percentage_and_picks_the_closest_limit() {
        // Session 85%, month 90%: the month is the more pressing.
        let spent = Spent { session_usd: 1.7, month_usd: 36.0, ..Default::default() };
        let (m, text) = limits().warning(&spent).unwrap();
        assert_eq!(m.period, Period::Month);
        assert!(text.starts_with("Monthly spending is at 90%"), "{text}");
    }

    #[test]
    fn under_the_warning_threshold_nothing_is_said() {
        let spent = Spent { session_usd: 1.0, day_usd: 1.0, month_usd: 1.0, ..Default::default() };
        assert!(limits().warning(&spent).is_none() && limits().breach(&spent).is_none());
    }

    #[test]
    fn a_run_in_progress_counts_against_every_window() {
        let spent = Spent { session_usd: 1.0, day_usd: 1.0, week_usd: 1.0, month_usd: 1.0 }.plus(0.5);
        assert_eq!(spent, Spent { session_usd: 1.5, day_usd: 1.5, week_usd: 1.5, month_usd: 1.5 });
    }

    #[test]
    fn concurrent_runs_cannot_together_pass_a_limit_neither_reached_alone() {
        // Day limit 5 EUR at 0.5 EUR/USD is 10 USD. Each run has spent 6 USD of its own; alone neither is over.
        let limits = Limits { session_eur: 0.0, ..limits() };
        let baseline = Spent::default();
        let flight = InFlight::default();
        let others_a = flight.report("a", 6.0);
        assert!(limits.breach(&baseline.plus_run(6.0, others_a)).is_none());
        let others_b = flight.report("b", 6.0);
        assert_eq!(others_b, 6.0);
        let (m, _) = limits.breach(&baseline.plus_run(6.0, others_b)).expect("together they are 12 USD");
        assert_eq!(m.period, Period::Day);
    }

    #[test]
    fn the_per_conversation_limit_ignores_what_other_runs_spend() {
        // Session limit 1 EUR = 2 USD: another run's 50 USD does not count against this conversation.
        let spent = Spent::default().plus_run(1.0, 50.0);
        assert!(limits().breach(&spent).map(|(m, _)| m.period) != Some(Period::Session));
        let own = Spent::default().plus_run(2.5, 0.0);
        assert_eq!(limits().breach(&own).unwrap().0.period, Period::Session);
    }

    #[test]
    fn a_run_that_ends_stops_counting_against_the_others() {
        let flight = InFlight::default();
        flight.report("a", 4.0);
        assert_eq!(flight.report("b", 1.0), 4.0);
        flight.finish("a");
        assert_eq!(flight.report("b", 2.0), 0.0);
        assert_eq!(flight.total(), 2.0);
        flight.finish("b");
        assert_eq!(flight.total(), 0.0);
    }

    #[test]
    fn a_run_reporting_again_replaces_its_figure_rather_than_adding() {
        let flight = InFlight::default();
        flight.report("a", 1.0);
        flight.report("a", 3.0);
        assert_eq!(flight.report("b", 0.0), 3.0);
    }

    #[test]
    fn a_nonsense_rate_cannot_switch_the_limits_off() {
        let mut l = limits();
        l.eur_per_usd = 0.0;
        let spent = Spent { session_usd: 1.5, ..Default::default() };
        assert!(l.breach(&spent).is_some(), "with no usable rate a dollar counts as a euro");
        l.eur_per_usd = f64::NAN;
        assert!(l.breach(&spent).is_some());
    }

    #[test]
    fn small_amounts_do_not_read_as_nothing() {
        assert_eq!(eur(0.0034), "€0.003");
        assert_eq!(eur(0.0), "€0.00");
        assert_eq!(eur(12.5), "€12.50");
    }
}
