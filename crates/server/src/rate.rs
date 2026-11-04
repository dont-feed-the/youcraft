//! How often one caller may change the site: a rate per account, or per address for those
//! signed out, kept by the generic cell rate algorithm.
//!
//! Each caller owes until an instant `due`. A change at `now` is allowed while `due - now`
//! stays within `(burst - 1) * interval`, and moves `due` to `max(due, now) + interval`: a bucket
//! of `burst` changes that refills one per `interval`, held as one instant, so it is exact. A
//! caller whose `due` has passed owes nothing and is forgotten; the table holds at most
//! [`CALLERS`] callers who owe, and while it is full a caller it does not hold waits.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use youcraft_api::AccountId;

use crate::config::WriteRate;
use crate::site::AddressKey;

/// The most callers the table remembers at once.
const CALLERS: usize = 1 << 16;

/// Whose rate a change spends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spender {
    /// A signed-in account, from any browser or program.
    Account(AccountId),
    /// Whoever is signed out at one address.
    Address(AddressKey),
}

/// The rate of every caller.
pub struct RateLimit {
    rate: WriteRate,
    due: Mutex<HashMap<Spender, Instant>>,
}

impl RateLimit {
    /// Callers limited to `rate`.
    pub fn new(rate: WriteRate) -> RateLimit {
        RateLimit {
            rate,
            due: Mutex::new(HashMap::new()),
        }
    }

    /// Spend one change of `spender` at `now`; the time until it may change the site again
    /// when it may not now.
    pub fn spend(&self, spender: Spender, now: Instant) -> Result<(), Duration> {
        let Some(interval) = self.rate.interval else {
            return Ok(());
        };
        let tolerance = interval.saturating_mul(self.rate.burst.saturating_sub(1));
        let mut due = self.due.lock().unwrap_or_else(PoisonError::into_inner);
        if due.len() >= CALLERS && !due.contains_key(&spender) {
            due.retain(|_, until| *until > now);
            if due.len() >= CALLERS {
                return Err(interval);
            }
        }
        let owed = due
            .get(&spender)
            .map_or(Duration::ZERO, |until| until.saturating_duration_since(now));
        if owed > tolerance {
            return Err(owed.saturating_sub(tolerance));
        }
        let until = now
            .checked_add(owed.saturating_add(interval))
            .ok_or(interval)?;
        due.insert(spender, until);
        Ok(())
    }
}

/// `wait` in whole seconds, rounded up, and at least one.
pub fn seconds(wait: Duration) -> u64 {
    let whole = wait.as_secs();
    whole
        .saturating_add(u64::from(wait.subsec_nanos() > 0))
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(burst: u32, interval: u64) -> RateLimit {
        RateLimit::new(WriteRate {
            burst,
            interval: Some(Duration::from_secs(interval)),
        })
    }

    #[test]
    fn a_burst_then_one_change_per_interval() {
        let rate = limit(3, 10);
        let ada = Spender::Account(AccountId::from_bits(1));
        let start = Instant::now();
        let at = |seconds| start + Duration::from_secs(seconds);
        for _ in 0..3 {
            assert_eq!(rate.spend(ada, start), Ok(()));
        }
        assert_eq!(rate.spend(ada, start), Err(Duration::from_secs(10)));
        assert_eq!(rate.spend(ada, at(4)), Err(Duration::from_secs(6)));
        assert_eq!(rate.spend(ada, at(10)), Ok(()), "one change forgiven");
        assert_eq!(rate.spend(ada, at(10)), Err(Duration::from_secs(10)));
        let bob = Spender::Account(AccountId::from_bits(2));
        assert_eq!(
            rate.spend(bob, at(10)),
            Ok(()),
            "each caller has their own rate"
        );
        for _ in 0..3 {
            assert_eq!(
                rate.spend(ada, at(100)),
                Ok(()),
                "a rested caller bursts again"
            );
        }
        assert!(rate.spend(ada, at(100)).is_err());
    }

    #[test]
    fn no_interval_is_no_limit() {
        let rate = RateLimit::new(WriteRate {
            burst: 1,
            interval: None,
        });
        let nobody = Spender::Address(AddressKey::NOBODY);
        for _ in 0..1000 {
            assert_eq!(rate.spend(nobody, Instant::now()), Ok(()));
        }
    }

    #[test]
    fn the_table_forgets_who_owes_nothing_and_never_outgrows_its_bound() {
        let rate = limit(1, 10);
        let start = Instant::now();
        let caller = |n: usize| {
            let mut key = [0; 32];
            key[..8].copy_from_slice(&(n as u64).to_be_bytes());
            Spender::Address(AddressKey(key))
        };
        for n in 0..CALLERS {
            assert_eq!(rate.spend(caller(n), start), Ok(()));
        }
        assert_eq!(
            rate.spend(caller(CALLERS), start),
            Err(Duration::from_secs(10)),
            "while every caller owes, a newcomer waits"
        );
        let later = start + Duration::from_secs(10);
        assert_eq!(
            rate.spend(caller(CALLERS), later),
            Ok(()),
            "once they are paid up, they go"
        );
        assert_eq!(rate.due.lock().unwrap().len(), 1);
    }

    #[test]
    fn waits_round_up_to_whole_seconds() {
        assert_eq!(seconds(Duration::from_millis(1)), 1);
        assert_eq!(seconds(Duration::from_millis(2500)), 3);
        assert_eq!(seconds(Duration::from_secs(4)), 4);
        assert_eq!(seconds(Duration::ZERO), 1);
    }
}
