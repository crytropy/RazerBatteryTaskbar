//! Windows USB/HID device-change callbacks for the native tray.
//! No HID operations happen inside the Windows callback.
use std::time::{Duration, Instant};

/// A receiver unplug or replug produces several HID interface events within a
/// few hundred milliseconds. The burst is collapsed into one rescan.
pub const DEVICE_CHANGE_DEBOUNCE: Duration = Duration::from_millis(1500);
/// After a replug the mouse still has to re-pair with the receiver, so the
/// first read may report `deviceNotResponding`. One follow-up rescan covers it.
pub const DEVICE_CHANGE_FOLLOW_UP: Duration = Duration::from_secs(3);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Burst(Instant),
    FollowUp(Instant),
}

/// Pure debounce state machine: the Windows callback feeds `on_event`, a single
/// timer thread feeds `on_timer`. Neither touches HID.
#[derive(Debug, Default)]
pub struct DeviceChangeDebouncer {
    phase: Phase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerOutcome {
    /// Sleep until the given instant, then call `on_timer` again.
    Wait(Instant),
    /// Request one rescan now; keep the timer alive if `wait_until` is set.
    Rescan { wait_until: Option<Instant> },
}

impl DeviceChangeDebouncer {
    /// Returns the deadline a new timer must wait for when no timer is running.
    pub fn on_event(&mut self, now: Instant) -> Option<Instant> {
        let deadline = now + DEVICE_CHANGE_DEBOUNCE;
        let arm_timer = matches!(self.phase, Phase::Idle);
        self.phase = Phase::Burst(deadline);
        arm_timer.then_some(deadline)
    }

    pub fn on_timer(&mut self, now: Instant) -> TimerOutcome {
        match self.phase {
            Phase::Idle => TimerOutcome::Rescan { wait_until: None },
            Phase::Burst(deadline) | Phase::FollowUp(deadline) if now < deadline => {
                TimerOutcome::Wait(deadline)
            }
            Phase::Burst(deadline) => {
                let follow_up = deadline + DEVICE_CHANGE_FOLLOW_UP;
                self.phase = Phase::FollowUp(follow_up);
                TimerOutcome::Rescan {
                    wait_until: Some(follow_up),
                }
            }
            Phase::FollowUp(_) => {
                self.phase = Phase::Idle;
                TimerOutcome::Rescan { wait_until: None }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_of_interface_events_schedules_one_rescan() {
        let mut debouncer = DeviceChangeDebouncer::default();
        let now = Instant::now();

        // The first event of a burst arms a timer; the rest only extend it.
        assert_eq!(debouncer.on_event(now), Some(now + DEVICE_CHANGE_DEBOUNCE));
        assert_eq!(debouncer.on_event(now + Duration::from_millis(50)), None);
        assert_eq!(debouncer.on_event(now + Duration::from_millis(400)), None);
        let last = now + Duration::from_millis(400);

        // Waking before the extended deadline keeps waiting.
        assert_eq!(
            debouncer.on_timer(now + DEVICE_CHANGE_DEBOUNCE),
            TimerOutcome::Wait(last + DEVICE_CHANGE_DEBOUNCE)
        );
        // At the deadline exactly one rescan is requested.
        assert!(matches!(
            debouncer.on_timer(last + DEVICE_CHANGE_DEBOUNCE),
            TimerOutcome::Rescan { .. }
        ));
        // Once the timer has run its course, a later event starts a new burst.
        assert_eq!(
            debouncer.on_timer(last + DEVICE_CHANGE_DEBOUNCE + DEVICE_CHANGE_FOLLOW_UP),
            TimerOutcome::Rescan { wait_until: None }
        );
        let later = now + Duration::from_secs(30);
        assert_eq!(
            debouncer.on_event(later),
            Some(later + DEVICE_CHANGE_DEBOUNCE)
        );
    }

    #[test]
    fn burst_rescan_is_followed_by_one_settle_rescan_unless_a_new_burst_starts() {
        let mut debouncer = DeviceChangeDebouncer::default();
        let now = Instant::now();
        let deadline = debouncer.on_event(now).unwrap();

        // The receiver needs a moment to re-pair with the mouse after it
        // reappears, so one follow-up rescan is scheduled after the burst.
        let follow_up = deadline + DEVICE_CHANGE_FOLLOW_UP;
        assert_eq!(
            debouncer.on_timer(deadline),
            TimerOutcome::Rescan {
                wait_until: Some(follow_up)
            }
        );
        assert_eq!(
            debouncer.on_timer(follow_up - Duration::from_millis(1)),
            TimerOutcome::Wait(follow_up)
        );
        assert_eq!(
            debouncer.on_timer(follow_up),
            TimerOutcome::Rescan { wait_until: None }
        );

        // Events while the follow-up is pending restart the burst instead of
        // arming a second timer.
        let deadline = debouncer.on_event(now).unwrap();
        assert_eq!(
            debouncer.on_timer(deadline),
            TimerOutcome::Rescan {
                wait_until: Some(deadline + DEVICE_CHANGE_FOLLOW_UP)
            }
        );
        let restart = deadline + Duration::from_secs(2);
        assert_eq!(debouncer.on_event(restart), None);
        assert_eq!(
            debouncer.on_timer(deadline + DEVICE_CHANGE_FOLLOW_UP),
            TimerOutcome::Wait(restart + DEVICE_CHANGE_DEBOUNCE)
        );
    }
}
