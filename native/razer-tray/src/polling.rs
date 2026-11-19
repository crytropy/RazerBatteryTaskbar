use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::Instant;

use razer_core::frontend::FrontendSnapshot;
use razer_core::notifications::{NotificationRequest, evaluate_events};
use razer_core::service::CoreService;
use razer_core::settings::{NotificationSettings, PrimaryDevicePreference};
use razer_core::transport::BatteryTransport;

use crate::refresh_schedule::{RecoveryOutcome, RefreshSchedule, ScheduleConfig};

/// Only frontend-safe data leaves the worker. Raw device IDs/serials stay inside CoreService.
#[derive(Debug)]
pub struct CoreUpdate {
    pub snapshot: FrontendSnapshot,
    pub notifications: Vec<NotificationRequest>,
}

enum WorkerCommand {
    Refresh,
    Resumed,
    DeviceChanged,
    SetNotifications(NotificationSettings),
    SetPrimaryDevice(PrimaryDevicePreference),
    Stop,
}

/// The worker owns the transport and serializes every HID read on its own
/// thread. It also owns the refresh schedule: callers only report what
/// happened (a wake, a device change, a manual refresh) and the worker
/// decides when to read.
pub struct CoreWorker {
    sender: Sender<WorkerCommand>,
}

impl CoreWorker {
    pub fn start<T, F>(
        transport: T,
        schedule: ScheduleConfig,
        notifications: NotificationSettings,
        primary_device: PrimaryDevicePreference,
        on_update: F,
    ) -> Self
    where
        T: BatteryTransport + Send + 'static,
        F: FnMut(CoreUpdate) -> bool + Send + 'static,
    {
        assert!(
            !schedule.poll_interval.is_zero(),
            "poll interval must be nonzero"
        );
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let mut core = CoreService::new(transport);
            core.set_primary_device_preference(primary_device);
            run_worker(core, receiver, schedule, notifications, on_update);
        });

        Self { sender }
    }

    pub fn request_refresh(&self) -> bool {
        self.sender.send(WorkerCommand::Refresh).is_ok()
    }

    /// Windows woke from sleep or hibernate: start post-resume recovery.
    pub fn resumed(&self) -> bool {
        self.sender.send(WorkerCommand::Resumed).is_ok()
    }

    /// A Razer device interface arrived or left: schedule a device-change rescan.
    pub fn device_changed(&self) -> bool {
        self.sender.send(WorkerCommand::DeviceChanged).is_ok()
    }

    pub fn set_notifications(&self, settings: NotificationSettings) -> bool {
        self.sender
            .send(WorkerCommand::SetNotifications(settings))
            .is_ok()
    }

    /// Re-point the primary device on the running core. The worker answers
    /// with a fresh snapshot built from the devices it already knows, so the
    /// icon switches at once and no extra HID handle is opened.
    pub fn set_primary_device(&self, preference: PrimaryDevicePreference) -> bool {
        self.sender
            .send(WorkerCommand::SetPrimaryDevice(preference))
            .is_ok()
    }
}

impl Drop for CoreWorker {
    fn drop(&mut self) {
        // Never block the event loop waiting for a device handle to finish closing.
        let _ = self.sender.send(WorkerCommand::Stop);
    }
}

/// Worker state other than the core itself.
struct WorkerState {
    schedule: RefreshSchedule,
    notifications: NotificationSettings,
    /// Publish the current snapshot; no HID read is needed for it.
    publish: bool,
}

impl WorkerState {
    /// Returns `false` when the worker should stop.
    fn apply<T: BatteryTransport>(
        &mut self,
        command: WorkerCommand,
        core: &mut CoreService<T>,
    ) -> bool {
        let now = Instant::now();
        match command {
            WorkerCommand::Refresh => self.schedule.refresh_requested(now),
            WorkerCommand::Resumed => self.schedule.resumed(now),
            WorkerCommand::DeviceChanged => self.schedule.device_changed(now),
            WorkerCommand::SetNotifications(next) => self.notifications = next,
            WorkerCommand::SetPrimaryDevice(preference) => {
                core.set_primary_device_preference(preference);
                self.publish = true;
            }
            WorkerCommand::Stop => return false,
        }

        true
    }
}

/// A snapshot is a fresh battery read only when the transport read succeeded
/// this time and the primary device answered with a battery level. A
/// `deviceNotResponding` failure keeps the previous battery value, so the
/// retained value alone is not enough.
fn is_fresh_battery_read(snapshot: &FrontendSnapshot) -> bool {
    snapshot.transport_healthy
        && snapshot
            .primary_device
            .as_ref()
            .is_some_and(|device| device.connected && device.battery.is_some())
}

fn run_worker<T, F>(
    mut core: CoreService<T>,
    receiver: Receiver<WorkerCommand>,
    schedule: ScheduleConfig,
    notifications: NotificationSettings,
    mut on_update: F,
) where
    T: BatteryTransport,
    F: FnMut(CoreUpdate) -> bool,
{
    let mut state = WorkerState {
        schedule: RefreshSchedule::new(schedule, Instant::now()),
        notifications,
        publish: false,
    };

    loop {
        // Coalesce commands accumulated during a HID read.
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    if !state.apply(command, &mut core) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if state.schedule.take_due(Instant::now()) {
            state.publish = false;
            let result = core.refresh();
            let snapshot = core.frontend_snapshot();
            let fresh = is_fresh_battery_read(&snapshot);
            if state.schedule.read_completed(Instant::now(), fresh) == RecoveryOutcome::Exhausted {
                eprintln!("post-resume recovery reads failed; regular polling continues");
            }
            let update = CoreUpdate {
                snapshot,
                notifications: evaluate_events(&result.events, &state.notifications),
            };

            if !on_update(update) {
                return;
            }
        } else if state.publish {
            state.publish = false;
            let update = CoreUpdate {
                snapshot: core.frontend_snapshot(),
                notifications: Vec::new(),
            };

            if !on_update(update) {
                return;
            }
        }

        // Wait for the next deadline, not a fixed interval, so commands that
        // do not read never push the next read back.
        let wait = state
            .schedule
            .next_deadline()
            .saturating_duration_since(Instant::now());
        match receiver.recv_timeout(wait) {
            Ok(command) => {
                if !state.apply(command, &mut core) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;
    use std::time::Duration;

    use super::*;
    use razer_core::state::{DeviceReading, DeviceType};
    use razer_core::transport::{TransportError, TransportErrorKind};

    struct TestTransport {
        reads: Arc<AtomicUsize>,
    }

    impl BatteryTransport for TestTransport {
        fn name(&self) -> &'static str {
            "worker-test"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
            Ok(vec![DeviceReading {
                vendor_id: 0x1532,
                product_id: 0x00B7,
                product_name: Some("DeathAdder V3 Pro".to_string()),
                device_type: DeviceType::Mouse,
                battery: Some(78.8),
                charging: None,
                serial_number: None,
            }])
        }
    }

    #[test]
    fn initial_hid_refresh_runs_off_the_caller_thread() {
        let caller = thread::current().id();
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::clone(&reads),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |update| {
                sender
                    .send((thread::current().id(), update.snapshot))
                    .is_ok()
            },
        );

        let (worker_thread, snapshot) = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_ne!(worker_thread, caller);
        assert_eq!(snapshot.primary_device.unwrap().battery, Some(78.8));
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        drop(worker);
    }

    #[test]
    fn rapid_refresh_requests_are_serialized_and_worker_stays_responsive() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::clone(&reads),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |_| sender.send(()).is_ok(),
        );

        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..10 {
            assert!(worker.request_refresh());
        }

        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        thread::sleep(Duration::from_millis(50));
        // A burst is coalesced instead of opening ten concurrent device handles.
        assert!(reads.load(Ordering::SeqCst) <= 3);
        drop(worker);
    }

    fn mouse() -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00B7,
            product_name: Some("DeathAdder V3 Pro".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(78.8),
            charging: None,
            serial_number: None,
        }
    }

    fn headset() -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x0527,
            product_name: Some("BlackShark V2 Pro".to_string()),
            device_type: DeviceType::Headset,
            battery: Some(41.0),
            charging: None,
            serial_number: None,
        }
    }

    /// Replays scripted scans in order and repeats the last one forever.
    struct ScriptedTransport {
        scans: Vec<Vec<DeviceReading>>,
        reads: Arc<AtomicUsize>,
    }

    impl BatteryTransport for ScriptedTransport {
        fn name(&self) -> &'static str {
            "worker-scripted"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            let index = self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.scans[index.min(self.scans.len() - 1)].clone())
        }
    }

    fn primary_product_id(snapshot: &FrontendSnapshot) -> Option<u16> {
        snapshot
            .primary_device
            .as_ref()
            .map(|device| device.product_id)
    }

    #[test]
    fn primary_device_change_republishes_the_snapshot_without_a_hid_read() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            ScriptedTransport {
                scans: vec![vec![mouse(), headset()]],
                reads: Arc::clone(&reads),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |update| sender.send(update.snapshot).is_ok(),
        );

        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x00B7),
            "automatic picks the mouse"
        );
        assert_eq!(snapshot.devices.len(), 2);

        assert!(worker.set_primary_device(PrimaryDevicePreference::ProductId(0x0527)));
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(primary_product_id(&snapshot), Some(0x0527));
        assert_eq!(
            reads.load(Ordering::SeqCst),
            1,
            "switching the primary device must not open another HID handle"
        );

        assert!(worker.set_primary_device(PrimaryDevicePreference::Auto));
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(primary_product_id(&snapshot), Some(0x00B7));
        drop(worker);
    }

    /// Ticket #9: the preferred device unplugged falls back to the core's
    /// default ordering (Mouse first) and comes back by itself on reconnect.
    #[test]
    fn preferred_device_offline_falls_back_to_default_ordering_and_returns_on_reconnect() {
        let (sender, receiver) = mpsc::channel();

        let worker = CoreWorker::start(
            ScriptedTransport {
                scans: vec![
                    vec![mouse(), headset()],
                    vec![mouse()],
                    vec![mouse(), headset()],
                ],
                reads: Arc::new(AtomicUsize::new(0)),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::ProductId(0x0527),
            move |update| sender.send(update.snapshot).is_ok(),
        );

        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x0527),
            "preferred headset wins"
        );

        assert!(worker.request_refresh());
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x00B7),
            "mouse is the fallback"
        );
        assert!(
            snapshot
                .devices
                .iter()
                .any(|device| device.product_id == 0x0527 && !device.connected),
            "the unplugged headset is still listed as disconnected"
        );

        assert!(worker.request_refresh());
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x0527),
            "preference is restored"
        );
        drop(worker);
    }

    fn test_config(poll_interval: Duration) -> ScheduleConfig {
        ScheduleConfig {
            poll_interval,
            recovery_offsets: [
                Duration::from_millis(100),
                Duration::from_millis(250),
                Duration::from_millis(400),
            ],
            debounce: Duration::from_millis(50),
            follow_up: Duration::from_millis(150),
        }
    }

    /// Replays scripted results in order and repeats the last one forever.
    struct ScriptedResults {
        results: Vec<Result<Vec<DeviceReading>, TransportError>>,
        reads: Arc<AtomicUsize>,
    }

    impl BatteryTransport for ScriptedResults {
        fn name(&self) -> &'static str {
            "worker-scripted-results"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            let index = self.reads.fetch_add(1, Ordering::SeqCst);
            self.results[index.min(self.results.len() - 1)].clone()
        }
    }

    fn mouse_at(battery: f32) -> Result<Vec<DeviceReading>, TransportError> {
        Ok(vec![DeviceReading {
            battery: Some(battery),
            ..mouse()
        }])
    }

    fn mouse_asleep() -> Result<Vec<DeviceReading>, TransportError> {
        Err(TransportError::with_kind(
            TransportErrorKind::DeviceNotResponding,
            "device busy",
        ))
    }

    #[test]
    fn a_wake_reads_at_the_recovery_offsets_until_a_fresh_battery_read() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            ScriptedResults {
                // Before sleep, then +100 ms still re-pairing, then +250 ms back.
                results: vec![mouse_at(80.0), mouse_asleep(), mouse_at(79.0)],
                reads: Arc::clone(&reads),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |update| sender.send(update.snapshot).is_ok(),
        );
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();

        assert!(worker.resumed());
        let asleep = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            !asleep.transport_healthy,
            "the mouse kept its old value only"
        );
        let recovered = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(recovered.primary_device.unwrap().battery, Some(79.0));

        // The +400 ms attempt was cancelled by the fresh read at +250 ms.
        thread::sleep(Duration::from_millis(400));
        assert!(receiver.try_recv().is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        drop(worker);
    }

    #[test]
    fn a_device_change_burst_reads_once_settled_and_once_after_the_follow_up() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::clone(&reads),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |_| sender.send(()).is_ok(),
        );
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();

        for _ in 0..6 {
            assert!(worker.device_changed());
        }
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();

        thread::sleep(Duration::from_millis(300));
        assert!(receiver.try_recv().is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 3, "six events cost two reads");
        drop(worker);
    }

    #[test]
    fn commands_that_do_not_read_leave_the_regular_poll_on_time() {
        let (sender, receiver) = mpsc::channel();

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::new(AtomicUsize::new(0)),
            },
            test_config(Duration::from_millis(300)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |_| sender.send(()).is_ok(),
        );
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();

        // A steady trickle of settings changes must not push the poll back.
        let started = std::time::Instant::now();
        while started.elapsed() < Duration::from_millis(700) {
            assert!(worker.set_notifications(NotificationSettings::default()));
            if receiver.try_recv().is_ok() {
                drop(worker);
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("the 300 ms poll never ran while settings kept changing");
    }

    /// Switching the primary device republishes a healthy-looking snapshot
    /// without a read; it must not end post-resume recovery.
    #[test]
    fn a_primary_device_switch_during_recovery_does_not_end_it() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            ScriptedResults {
                results: vec![mouse_at(80.0), mouse_asleep()],
                reads: Arc::clone(&reads),
            },
            test_config(Duration::from_secs(60)),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |_| sender.send(()).is_ok(),
        );
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();

        assert!(worker.resumed());
        assert!(worker.set_primary_device(PrimaryDevicePreference::Auto));
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();

        // All three attempts still run: +100, +250 and +400 ms.
        thread::sleep(Duration::from_millis(700));
        assert_eq!(reads.load(Ordering::SeqCst), 4);
        drop(worker);
    }
}
