//! The tray's refresh schedule: when the worker next reads the battery.
//!
//! Three rule sets share one schedule: the regular poll, post-resume recovery
//! and device-change rescans. It is a pure state machine owned by the worker:
//! it never touches HID, threads or clocks, and receives the current instant
//! with every call.

use std::time::{Duration, Instant};

/// The wireless mouse and its receiver need several seconds to re-pair after
/// sleep, so post-resume recovery reads at these offsets from the wake.
const RECOVERY_OFFSETS: [Duration; 3] = [
    Duration::from_secs(3),
    Duration::from_secs(10),
    Duration::from_secs(20),
];
/// A receiver unplug or replug produces several HID interface events within a
/// few hundred milliseconds; the burst is read once it has been quiet this long.
const DEVICE_CHANGE_DEBOUNCE: Duration = Duration::from_millis(1500);
/// After a replug the mouse still has to re-pair with the receiver, so one
/// follow-up read comes this long after the burst settled.
const DEVICE_CHANGE_FOLLOW_UP: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleConfig {
    pub poll_interval: Duration,
    pub recovery_offsets: [Duration; 3],
    pub debounce: Duration,
    pub follow_up: Duration,
}

impl ScheduleConfig {
    /// The tray's production timings around the given regular poll interval.
    pub const fn tray(poll_interval: Duration) -> Self {
        Self {
            poll_interval,
            recovery_offsets: RECOVERY_OFFSETS,
            debounce: DEVICE_CHANGE_DEBOUNCE,
            follow_up: DEVICE_CHANGE_FOLLOW_UP,
        }
    }
}

/// What a completed read meant for post-resume recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// No recovery is in progress.
    Idle,
    /// Recovery continues; more attempts are scheduled.
    Pending,
    /// A fresh battery read arrived; the remaining attempts are cancelled.
    Recovered,
    /// The last attempt failed; the regular poll takes over.
    Exhausted,
}

#[derive(Debug)]
struct Recovery {
    resumed_at: Instant,
    /// Index of the next attempt in `recovery_offsets`; equal to its length
    /// once the last attempt has been taken.
    next_attempt: usize,
}

/// Device-change rescan: a burst of events settles into one read, followed by
/// one more read for the mouse to re-pair.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Rescan {
    #[default]
    Idle,
    /// Read once the burst has been quiet until this instant.
    Burst(Instant),
    /// The follow-up read is due at this instant.
    FollowUp(Instant),
}

#[derive(Debug)]
pub struct RefreshSchedule {
    config: ScheduleConfig,
    next_poll: Instant,
    recovery: Option<Recovery>,
    rescan: Rescan,
    /// A manual refresh waiting to be read, stamped with when it was asked for.
    requested: Option<Instant>,
}

impl RefreshSchedule {
    /// The first read is due at `now`.
    pub fn new(config: ScheduleConfig, now: Instant) -> Self {
        Self {
            config,
            next_poll: now,
            recovery: None,
            rescan: Rescan::Idle,
            requested: None,
        }
    }

    /// A Razer device interface arrived or left at `now`. Each event pushes
    /// the burst's settle time back; an event while the follow-up waits
    /// starts a new burst.
    pub fn device_changed(&mut self, now: Instant) {
        self.rescan = Rescan::Burst(now + self.config.debounce);
    }

    /// The user asked for a read at `now`.
    pub fn refresh_requested(&mut self, now: Instant) {
        self.requested = Some(now);
    }

    /// Windows woke at `now`: start post-resume recovery from its first
    /// attempt, replacing any recovery in progress.
    pub fn resumed(&mut self, now: Instant) {
        self.recovery = Some(Recovery {
            resumed_at: now,
            next_attempt: 0,
        });
    }

    /// Whether a read is due at `now`. Every trigger that is due is consumed,
    /// so triggers falling due together cost one read.
    pub fn take_due(&mut self, now: Instant) -> bool {
        let mut due = now >= self.next_poll;

        if self.requested.is_some_and(|at| now >= at) {
            self.requested = None;
            due = true;
        }

        if let Some(recovery) = self.recovery.as_mut() {
            while let Some(at) = recovery.next_attempt_at(&self.config.recovery_offsets) {
                if now < at {
                    break;
                }
                recovery.next_attempt += 1;
                due = true;
            }
        }

        match self.rescan {
            Rescan::Burst(settled) if now >= settled => {
                self.rescan = Rescan::FollowUp(settled + self.config.follow_up);
                due = true;
            }
            Rescan::FollowUp(at) if now >= at => {
                self.rescan = Rescan::Idle;
                due = true;
            }
            _ => {}
        }

        due
    }

    /// A read finished at `now`; `fresh` says whether it was a fresh battery read.
    pub fn read_completed(&mut self, now: Instant, fresh: bool) -> RecoveryOutcome {
        self.next_poll = now + self.config.poll_interval;

        let Some(recovery) = self.recovery.as_ref() else {
            return RecoveryOutcome::Idle;
        };

        if fresh {
            self.recovery = None;
            RecoveryOutcome::Recovered
        } else if recovery
            .next_attempt_at(&self.config.recovery_offsets)
            .is_none()
        {
            self.recovery = None;
            RecoveryOutcome::Exhausted
        } else {
            RecoveryOutcome::Pending
        }
    }

    /// The earliest instant at which a read becomes due.
    pub fn next_deadline(&self) -> Instant {
        let recovery = self
            .recovery
            .as_ref()
            .and_then(|recovery| recovery.next_attempt_at(&self.config.recovery_offsets));

        let rescan = match self.rescan {
            Rescan::Idle => None,
            Rescan::Burst(at) | Rescan::FollowUp(at) => Some(at),
        };

        [Some(self.next_poll), self.requested, recovery, rescan]
            .into_iter()
            .flatten()
            .min()
            .expect("the regular poll is always scheduled")
    }
}

impl Recovery {
    /// When the next attempt is due, if any is left.
    fn next_attempt_at(&self, offsets: &[Duration]) -> Option<Instant> {
        offsets
            .get(self.next_attempt)
            .map(|offset| self.resumed_at + *offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn secs(value: u64) -> Duration {
        Duration::from_secs(value)
    }

    fn config() -> ScheduleConfig {
        ScheduleConfig {
            poll_interval: secs(10),
            recovery_offsets: [secs(3), secs(10), secs(20)],
            debounce: ms(1500),
            follow_up: secs(3),
        }
    }

    #[test]
    fn the_first_read_is_due_at_once_and_the_next_poll_one_interval_after_a_read() {
        let t0 = Instant::now();
        let mut schedule = RefreshSchedule::new(config(), t0);
        assert!(schedule.take_due(t0));

        // A HID read takes about a second; the countdown starts when it ends.
        let read_end = t0 + ms(1200);
        schedule.read_completed(read_end, true);
        assert_eq!(schedule.next_deadline(), read_end + secs(10));
        assert!(!schedule.take_due(t0 + secs(10)));
        assert!(schedule.take_due(read_end + secs(10)));
    }

    /// A schedule that has done its first read at `t0` and sits idle.
    fn idle_schedule(t0: Instant) -> RefreshSchedule {
        let mut schedule = RefreshSchedule::new(config(), t0);
        assert!(schedule.take_due(t0));
        schedule.read_completed(t0, true);
        schedule
    }

    #[test]
    fn post_resume_recovery_reads_at_its_offsets_and_stops_at_the_first_fresh_read() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        let wake = t0 + secs(5);
        schedule.resumed(wake);

        assert_eq!(schedule.next_deadline(), wake + secs(3));
        assert!(!schedule.take_due(wake + ms(2999)));
        assert!(schedule.take_due(wake + secs(3)));
        // The receiver is still re-pairing: the mouse keeps its old battery value.
        assert_eq!(
            schedule.read_completed(wake + secs(4), false),
            RecoveryOutcome::Pending
        );

        assert_eq!(schedule.next_deadline(), wake + secs(10));
        assert!(schedule.take_due(wake + secs(10)));
        assert_eq!(
            schedule.read_completed(wake + secs(11), true),
            RecoveryOutcome::Recovered
        );

        // The +20 s attempt is cancelled; only the regular poll is left.
        assert!(!schedule.take_due(wake + secs(20)));
        assert_eq!(schedule.next_deadline(), wake + secs(11) + secs(10));
    }

    #[test]
    fn when_the_last_recovery_attempt_fails_the_regular_poll_takes_over() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        schedule.resumed(t0);

        for (offset, outcome) in [
            (secs(3), RecoveryOutcome::Pending),
            (secs(10), RecoveryOutcome::Pending),
            (secs(20), RecoveryOutcome::Exhausted),
        ] {
            assert!(schedule.take_due(t0 + offset));
            assert_eq!(schedule.read_completed(t0 + offset, false), outcome);
        }

        assert_eq!(schedule.next_deadline(), t0 + secs(30));
        assert_eq!(
            schedule.read_completed(t0 + secs(30), false),
            RecoveryOutcome::Idle
        );
    }

    #[test]
    fn a_new_wake_during_recovery_restarts_it_from_the_first_attempt() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        schedule.resumed(t0);
        assert!(schedule.take_due(t0 + secs(3)));
        assert_eq!(
            schedule.read_completed(t0 + secs(3), false),
            RecoveryOutcome::Pending
        );

        let second_wake = t0 + secs(8);
        schedule.resumed(second_wake);
        assert_eq!(schedule.next_deadline(), second_wake + secs(3));
        // The first wake's +10 s attempt no longer exists.
        assert!(!schedule.take_due(t0 + secs(10)));
        assert!(schedule.take_due(second_wake + secs(3)));
    }

    #[test]
    fn a_device_change_burst_costs_one_rescan_and_one_follow_up_whatever_it_reads() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        for offset in [ms(1000), ms(1050), ms(1400)] {
            schedule.device_changed(t0 + offset);
        }

        // The burst settles 1.5 s after its last event.
        let settled = t0 + ms(1400) + ms(1500);
        assert_eq!(schedule.next_deadline(), settled);
        assert!(!schedule.take_due(settled - ms(1)));
        assert!(schedule.take_due(settled));
        // A fresh read does not cancel the follow-up.
        schedule.read_completed(settled, true);

        let follow_up = settled + secs(3);
        assert_eq!(schedule.next_deadline(), follow_up);
        assert!(schedule.take_due(follow_up));
        schedule.read_completed(follow_up, true);

        assert_eq!(schedule.next_deadline(), follow_up + secs(10));
    }

    #[test]
    fn a_device_change_while_the_follow_up_waits_starts_a_new_burst() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        schedule.device_changed(t0);
        let settled = t0 + ms(1500);
        assert!(schedule.take_due(settled));
        schedule.read_completed(settled, true);

        let replug = settled + secs(2);
        schedule.device_changed(replug);
        // The pending follow-up gives way to the new burst.
        assert!(!schedule.take_due(settled + secs(3)));
        assert_eq!(schedule.next_deadline(), replug + ms(1500));
    }

    #[test]
    fn a_manual_refresh_is_due_at_once_and_counts_once() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        let click = t0 + secs(4);
        schedule.refresh_requested(click);

        assert_eq!(schedule.next_deadline(), click);
        assert!(schedule.take_due(click));
        assert!(!schedule.take_due(click), "the request is consumed");
        schedule.read_completed(click, true);
        assert_eq!(schedule.next_deadline(), click + secs(10));
    }

    #[test]
    fn a_fresh_rescan_read_during_recovery_ends_recovery() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        schedule.resumed(t0);
        schedule.device_changed(t0 + secs(1));

        let settled = t0 + ms(2500);
        assert!(schedule.take_due(settled));
        assert_eq!(
            schedule.read_completed(settled, true),
            RecoveryOutcome::Recovered
        );
        // The +3 s recovery attempt is gone; the rescan follow-up stays.
        assert_eq!(schedule.next_deadline(), settled + secs(3));
    }

    #[test]
    fn triggers_falling_due_together_cost_one_read() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        schedule.resumed(t0);
        schedule.device_changed(t0 + ms(1500));
        schedule.refresh_requested(t0 + secs(3));

        // Recovery's +3 s attempt, the settled burst and the manual request.
        assert!(schedule.take_due(t0 + secs(3)));
        assert!(!schedule.take_due(t0 + secs(3)));
        assert_eq!(
            schedule.read_completed(t0 + secs(3), false),
            RecoveryOutcome::Pending
        );
        assert_eq!(schedule.next_deadline(), t0 + secs(6), "rescan follow-up");
    }

    #[test]
    fn every_read_restarts_the_poll_countdown() {
        let t0 = Instant::now();
        let mut schedule = idle_schedule(t0);
        schedule.refresh_requested(t0 + secs(7));
        assert!(schedule.take_due(t0 + secs(7)));
        schedule.read_completed(t0 + secs(8), true);

        assert!(!schedule.take_due(t0 + secs(10)));
        assert_eq!(schedule.next_deadline(), t0 + secs(18));
    }
}
