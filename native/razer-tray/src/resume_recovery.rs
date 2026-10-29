//! Staggered battery refreshes after a Windows resume.
//!
//! The wireless mouse and its receiver need several seconds to re-pair after
//! sleep, so a single immediate refresh usually fails. This module schedules a
//! short series of recovery refreshes and stops as soon as one of them yields a
//! real battery read. It is a pure state machine driven by the tray event loop:
//! it never touches HID and never blocks.

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use razer_core::frontend::FrontendSnapshot;

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Mints a fresh generation for a resume that was just observed.
pub fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1
}

/// Delivers the recovery ticks for `generation` from a sleeping helper thread,
/// one per [`RECOVERY_DELAYS`] entry. `deliver` runs on that thread and must
/// only hand the tick to the event loop; the scheduler decides on the UI
/// thread whether the tick still matters.
pub fn spawn_ticks<F>(generation: u64, deliver: F)
where
    F: Fn(RecoveryTick) + Send + 'static,
{
    let spawned = thread::Builder::new()
        .name("razer-resume-recovery".to_string())
        .spawn(move || {
            let mut elapsed = Duration::ZERO;
            for tick in RecoveryTick::series(generation) {
                let Some(delay) = tick.delay() else {
                    continue;
                };
                thread::sleep(delay.saturating_sub(elapsed));
                elapsed = delay;
                deliver(tick);
            }
        });

    if let Err(error) = spawned {
        eprintln!("resume recovery thread unavailable: {error}");
    }
}

/// Offsets, measured from the resume event, at which recovery refreshes run.
pub const RECOVERY_DELAYS: [Duration; 3] = [
    Duration::from_secs(3),
    Duration::from_secs(10),
    Duration::from_secs(20),
];

/// One scheduled recovery refresh. `generation` identifies the resume that
/// produced it so ticks from a superseded resume are ignored. `attempt`
/// indexes [`RECOVERY_DELAYS`]; only [`RecoveryTick::series`] mints ticks,
/// so it is always in range, and the accessors tolerate a stray value anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTick {
    pub generation: u64,
    attempt: usize,
}

impl RecoveryTick {
    /// The full set of ticks for one resume, in firing order.
    pub fn series(generation: u64) -> Vec<Self> {
        (0..RECOVERY_DELAYS.len())
            .map(|attempt| Self {
                generation,
                attempt,
            })
            .collect()
    }

    /// Offset from the resume event, or `None` for an attempt outside the
    /// schedule (such a tick is ignored rather than panicking).
    pub fn delay(&self) -> Option<Duration> {
        RECOVERY_DELAYS.get(self.attempt).copied()
    }

    /// Whether this is the final attempt of its series.
    pub fn is_last(&self) -> bool {
        self.attempt + 1 == RECOVERY_DELAYS.len()
    }
}

/// What a battery update meant for the recovery that was in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// No recovery is in progress; this was an ordinary poll.
    Idle,
    /// Recovery continues; more attempts are scheduled.
    Pending,
    /// A real battery read arrived; the remaining attempts are cancelled.
    Recovered,
    /// Every attempt failed; ordinary scheduled polling takes over.
    Exhausted,
}

#[derive(Debug, Default)]
pub struct ResumeRecovery {
    active: Option<ActiveRecovery>,
}

#[derive(Debug)]
struct ActiveRecovery {
    generation: u64,
    last_attempt_fired: bool,
}

impl ResumeRecovery {
    /// Starts a recovery set for the given resume, replacing any earlier one.
    pub fn on_resume(&mut self, generation: u64) {
        self.active = Some(ActiveRecovery {
            generation,
            last_attempt_fired: false,
        });
    }

    /// Returns `true` when the tick should trigger a refresh now. Ticks from a
    /// superseded resume, or arriving after recovery finished, are dropped.
    pub fn on_tick(&mut self, tick: RecoveryTick) -> bool {
        let Some(active) = self.active.as_mut() else {
            return false;
        };
        if active.generation != tick.generation || tick.delay().is_none() {
            return false;
        }
        if tick.is_last() {
            active.last_attempt_fired = true;
        }
        true
    }

    /// Observes every snapshot the worker produces, recovery-triggered or not.
    pub fn on_update(&mut self, snapshot: &FrontendSnapshot) -> RecoveryOutcome {
        let Some(active) = self.active.as_ref() else {
            return RecoveryOutcome::Idle;
        };

        if has_fresh_battery_read(snapshot) {
            self.active = None;
            RecoveryOutcome::Recovered
        } else if active.last_attempt_fired {
            self.active = None;
            RecoveryOutcome::Exhausted
        } else {
            RecoveryOutcome::Pending
        }
    }

    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

/// A snapshot counts as a real battery read only when the transport read
/// succeeded this time and the primary device answered with a battery level.
/// A `deviceNotResponding` failure keeps the previous battery value, so the
/// retained value alone is not enough.
fn has_fresh_battery_read(snapshot: &FrontendSnapshot) -> bool {
    snapshot.transport_healthy
        && snapshot
            .primary_device
            .as_ref()
            .is_some_and(|device| device.connected && device.battery.is_some())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use razer_core::service::CoreService;
    use razer_core::state::{DeviceReading, DeviceType};
    use razer_core::transport::{BatteryTransport, TransportError, TransportErrorKind};

    use super::*;

    struct ScriptedTransport {
        results: VecDeque<Result<Vec<DeviceReading>, TransportError>>,
    }

    impl ScriptedTransport {
        fn new(results: Vec<Result<Vec<DeviceReading>, TransportError>>) -> Self {
            Self {
                results: results.into(),
            }
        }
    }

    impl BatteryTransport for ScriptedTransport {
        fn name(&self) -> &'static str {
            "scripted"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            self.results
                .pop_front()
                .expect("test script should cover every refresh")
        }
    }

    fn mouse_reading(battery: f32) -> Result<Vec<DeviceReading>, TransportError> {
        Ok(vec![DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00B7,
            product_name: Some("Razer DeathAdder V3 Pro".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(battery),
            charging: Some(false),
            serial_number: None,
        }])
    }

    fn mouse_asleep() -> Result<Vec<DeviceReading>, TransportError> {
        Err(TransportError::with_kind(
            TransportErrorKind::DeviceNotResponding,
            "device busy",
        ))
    }

    /// Drives one recovery tick the way the event loop does: deliver the tick,
    /// and if the scheduler asks for a refresh, run it on the core and feed the
    /// resulting snapshot back.
    fn drive_tick(
        recovery: &mut ResumeRecovery,
        core: &mut CoreService<ScriptedTransport>,
        tick: RecoveryTick,
    ) -> Option<RecoveryOutcome> {
        if !recovery.on_tick(tick) {
            return None;
        }
        core.refresh();
        Some(recovery.on_update(&core.frontend_snapshot()))
    }

    #[test]
    fn second_recovery_attempt_success_stops_remaining_refreshes() {
        let transport = ScriptedTransport::new(vec![
            mouse_reading(80.0), // last poll before sleep
            mouse_asleep(),      // +3 s: receiver still re-pairing
            mouse_reading(79.0), // +10 s: real battery read
        ]);
        let mut core = CoreService::new(transport);
        core.refresh();

        let mut recovery = ResumeRecovery::default();
        recovery.on_resume(1);

        let ticks = RecoveryTick::series(1);
        assert_eq!(ticks.len(), RECOVERY_DELAYS.len());

        assert_eq!(
            drive_tick(&mut recovery, &mut core, ticks[0]),
            Some(RecoveryOutcome::Pending)
        );
        assert_eq!(
            drive_tick(&mut recovery, &mut core, ticks[1]),
            Some(RecoveryOutcome::Recovered)
        );
        assert_eq!(drive_tick(&mut recovery, &mut core, ticks[2]), None);
        assert!(!recovery.is_active());

        let snapshot = core.frontend_snapshot();
        assert_eq!(snapshot.poll_statistics.attempts, 3);
        assert_eq!(snapshot.primary_device.unwrap().battery, Some(79.0));
    }

    #[test]
    fn all_recovery_attempts_fail_then_regular_polling_keeps_connected_and_last_battery() {
        let transport = ScriptedTransport::new(vec![
            mouse_reading(72.0), // last poll before sleep
            mouse_asleep(),      // +3 s
            mouse_asleep(),      // +10 s
            mouse_asleep(),      // +20 s
            mouse_reading(71.0), // next regular 60 s poll
        ]);
        let mut core = CoreService::new(transport);
        core.refresh();

        let mut recovery = ResumeRecovery::default();
        recovery.on_resume(7);

        let ticks = RecoveryTick::series(7);
        assert_eq!(
            drive_tick(&mut recovery, &mut core, ticks[0]),
            Some(RecoveryOutcome::Pending)
        );
        assert_eq!(
            drive_tick(&mut recovery, &mut core, ticks[1]),
            Some(RecoveryOutcome::Pending)
        );
        assert_eq!(
            drive_tick(&mut recovery, &mut core, ticks[2]),
            Some(RecoveryOutcome::Exhausted)
        );
        assert!(!recovery.is_active());

        let after_recovery = core.frontend_snapshot();
        let primary = after_recovery.primary_device.expect("mouse stays known");
        assert!(primary.connected);
        assert_eq!(primary.battery, Some(72.0));
        assert!(!after_recovery.transport_healthy);

        // The regular poll is not part of the recovery set.
        core.refresh();
        let regular = core.frontend_snapshot();
        assert_eq!(recovery.on_update(&regular), RecoveryOutcome::Idle);
        assert_eq!(regular.primary_device.unwrap().battery, Some(71.0));
    }

    #[test]
    fn a_later_resume_replaces_the_set_and_drops_stale_ticks() {
        let mut recovery = ResumeRecovery::default();

        recovery.on_resume(1);
        let first = RecoveryTick::series(1);
        assert!(recovery.on_tick(first[0]));

        // Not deduplicated by the 4 s power filter (e.g. a second wake much later).
        recovery.on_resume(2);
        let second = RecoveryTick::series(2);
        assert!(!recovery.on_tick(first[1]));
        assert!(!recovery.on_tick(first[2]));
        assert!(recovery.on_tick(second[0]));
        assert!(recovery.is_active());
    }

    #[test]
    fn an_out_of_range_attempt_has_no_delay_and_never_refreshes() {
        // Only `series` mints ticks, but a stray value must not panic the
        // helper thread or trigger a refresh.
        let stray = RecoveryTick {
            generation: 1,
            attempt: RECOVERY_DELAYS.len(),
        };
        assert_eq!(stray.delay(), None);
        assert!(!stray.is_last());

        let mut recovery = ResumeRecovery::default();
        recovery.on_resume(1);
        assert!(!recovery.on_tick(stray));
        assert!(recovery.is_active());

        let series = RecoveryTick::series(1);
        assert_eq!(
            series.iter().map(RecoveryTick::delay).collect::<Vec<_>>(),
            RECOVERY_DELAYS
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>()
        );
        assert!(series.last().unwrap().is_last());
        assert!(!series[0].is_last());
    }

    #[test]
    fn ticks_without_a_resume_never_refresh() {
        let mut recovery = ResumeRecovery::default();
        assert!(!recovery.on_tick(RecoveryTick::series(1)[0]));
    }
}
