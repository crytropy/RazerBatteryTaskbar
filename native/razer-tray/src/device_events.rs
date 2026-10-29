//! Windows USB/HID device-change callbacks for the native tray.
//! No HID operations happen inside the Windows callback.
//!
//! `CM_Register_Notification` (cfgmgr32) needs no window: like the power
//! callbacks it runs on a system thread. The callback only feeds the pure
//! [`DeviceChangeDebouncer`]; a single timer thread turns the debounced burst
//! into rescan requests for the background worker.
use std::ffi::c_void;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use std::{ptr, slice, thread};

use razer_core::RAZER_VENDOR_ID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
    CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER,
    CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CM_Register_Notification, CM_Unregister_Notification,
    CR_SUCCESS, HCMNOTIFICATION,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;

/// A receiver unplug or replug produces several HID interface events within a
/// few hundred milliseconds. The burst is collapsed into one rescan.
pub const DEVICE_CHANGE_DEBOUNCE: Duration = Duration::from_millis(1500);
/// After a replug the mouse still has to re-pair with the receiver, so the
/// first read may report `deviceNotResponding`. One follow-up rescan covers it.
pub const DEVICE_CHANGE_FOLLOW_UP: Duration = Duration::from_secs(3);

/// Device interface symbolic links look like
/// `\?\HID#VID_1532&PID_00B7&MI_00#...#{4d1e55b2-...}`. Only Razer interfaces
/// are worth a rescan; an unreadable link is treated as relevant so a real
/// change is never silenced. The tray does not parse the Razer protocol here,
/// it only recognizes the vendor ID in the device path.
pub fn is_razer_interface(symbolic_link: &str) -> bool {
    symbolic_link.is_empty()
        || symbolic_link
            .to_ascii_lowercase()
            .contains(&format!("vid_{RAZER_VENDOR_ID:04x}"))
}

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

    /// The timer `on_event` asked for could not be started: go back to idle so
    /// the next event arms a new one instead of waiting for a timer that will
    /// never fire.
    pub fn timer_unavailable(&mut self) {
        self.phase = Phase::Idle;
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

/// Starts the timer body on its own thread. Injected so the spawn-failure
/// path can be exercised without exhausting the process's threads.
type TimerSpawner = Box<dyn Fn(Box<dyn FnOnce() + Send>) -> io::Result<()> + Send + Sync>;

fn spawn_timer_thread(timer: Box<dyn FnOnce() + Send>) -> io::Result<()> {
    thread::Builder::new()
        .name("razer-device-change".to_string())
        .spawn(timer)
        .map(drop)
}

struct CallbackState {
    callback: Box<dyn Fn() + Send + Sync>,
    debouncer: Mutex<DeviceChangeDebouncer>,
    spawn_timer: TimerSpawner,
}

impl CallbackState {
    fn new(callback: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_spawner(callback, Box::new(spawn_timer_thread))
    }

    fn with_spawner(
        callback: impl Fn() + Send + Sync + 'static,
        spawn_timer: TimerSpawner,
    ) -> Self {
        Self {
            callback: Box::new(callback),
            debouncer: Mutex::new(DeviceChangeDebouncer::default()),
            spawn_timer,
        }
    }

    /// The debouncer holds plain state and nothing panics while it is locked,
    /// so a poisoned lock is recovered rather than silencing device changes.
    fn debouncer(&self) -> MutexGuard<'_, DeviceChangeDebouncer> {
        self.debouncer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Feeds one relevant interface event into the debouncer and starts the
    /// timer thread when none is running. Never blocks for long and never
    /// touches HID: the callback runs on a Windows thread-pool thread.
    fn on_event(self: &Arc<Self>) {
        let Some(deadline) = self.debouncer().on_event(Instant::now()) else {
            return;
        };

        let state = Arc::clone(self);
        if let Err(error) = (self.spawn_timer)(Box::new(move || state.run_timer(deadline))) {
            // Without a timer the burst would stay armed forever and every
            // later event would be swallowed; fall back to idle instead.
            eprintln!("device-change timer thread unavailable: {error}");
            self.debouncer().timer_unavailable();
        }
    }

    fn run_timer(&self, mut deadline: Instant) {
        loop {
            if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                thread::sleep(remaining);
            }

            let outcome = self.debouncer().on_timer(Instant::now());

            match outcome {
                TimerOutcome::Wait(next) => deadline = next,
                TimerOutcome::Rescan { wait_until } => {
                    (self.callback)();
                    match wait_until {
                        Some(next) => deadline = next,
                        None => return,
                    }
                }
            }
        }
    }
}

/// Copies the NUL-terminated UTF-16 symbolic link out of the event payload,
/// bounded by the size Windows reported.
unsafe fn symbolic_link(eventdata: *const CM_NOTIFY_EVENT_DATA, eventdatasize: u32) -> String {
    if eventdata.is_null() {
        return String::new();
    }

    // SAFETY: the caller guarantees eventdata points at eventdatasize valid
    // bytes for the duration of the callback.
    unsafe {
        let link = ptr::addr_of!((*eventdata).u.DeviceInterface.SymbolicLink).cast::<u16>();
        let offset = link as usize - eventdata as usize;
        let capacity = (eventdatasize as usize).saturating_sub(offset) / 2;
        let units = slice::from_raw_parts(link, capacity);
        let end = units.iter().position(|unit| *unit == 0).unwrap_or(capacity);
        String::from_utf16_lossy(&units[..end])
    }
}

unsafe extern "system" fn device_change_callback(
    _notify: HCMNOTIFICATION,
    context: *const c_void,
    action: CM_NOTIFY_ACTION,
    eventdata: *const CM_NOTIFY_EVENT_DATA,
    eventdatasize: u32,
) -> u32 {
    if context.is_null() {
        return 0;
    }
    if action != CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL
        && action != CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL
    {
        return 0;
    }

    // SAFETY: registration leaked an Arc<CallbackState> into the context; it
    // is reclaimed only after Windows has unregistered the callback. The
    // ManuallyDrop keeps this borrow from decrementing the count.
    let state =
        std::mem::ManuallyDrop::new(unsafe { Arc::from_raw(context.cast::<CallbackState>()) });

    // SAFETY: Windows owns eventdata for the duration of this call.
    if is_razer_interface(&unsafe { symbolic_link(eventdata, eventdatasize) }) {
        state.on_event();
    }

    0
}

pub struct DeviceChangeNotifications {
    registration: HCMNOTIFICATION,
    context: *const CallbackState,
}

impl DeviceChangeNotifications {
    /// `callback` is invoked once per debounced rescan request, on the timer
    /// thread; it should only enqueue work for the worker or event loop.
    pub fn register(callback: impl Fn() + Send + Sync + 'static) -> io::Result<Self> {
        let context = Arc::into_raw(Arc::new(CallbackState::new(callback)));

        let mut filter = CM_NOTIFY_FILTER {
            cbSize: size_of::<CM_NOTIFY_FILTER>() as u32,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            ..Default::default()
        };
        filter.u.DeviceInterface.ClassGuid = GUID_DEVINTERFACE_HID;
        let mut registration: HCMNOTIFICATION = ptr::null_mut();

        // SAFETY: filter outlives the call; the context stays alive until
        // CM_Unregister_Notification has returned.
        let result = unsafe {
            CM_Register_Notification(
                &filter,
                context.cast(),
                Some(device_change_callback),
                &mut registration,
            )
        };

        if result != CR_SUCCESS {
            // SAFETY: Windows did not keep the context on failure.
            unsafe { drop(Arc::from_raw(context)) };
            return Err(io::Error::other(format!(
                "CM_Register_Notification failed with CONFIGRET {result:#x}"
            )));
        }

        Ok(Self {
            registration,
            context,
        })
    }
}

impl Drop for DeviceChangeNotifications {
    fn drop(&mut self) {
        // SAFETY: unregistering blocks until in-flight callbacks return, so
        // the context is released only after Windows stopped using it. A timer
        // thread may still hold its own Arc clone; it keeps the state alive.
        unsafe {
            let _ = CM_Unregister_Notification(self.registration);
            drop(Arc::from_raw(self.context));
        }
    }
}

// SAFETY: the raw pointers are only handles into Send + Sync state.
unsafe impl Send for DeviceChangeNotifications {}

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

    #[test]
    fn only_razer_hid_interfaces_trigger_a_rescan() {
        assert!(is_razer_interface(
            r"\?\HID#VID_1532&PID_00B7&MI_00#7&2a3b4c5d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
        ));
        assert!(is_razer_interface(
            r"\?\HID#vid_1532&pid_00b6&mi_00#8&1&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
        ));
        assert!(!is_razer_interface(
            r"\?\HID#VID_046D&PID_C52B&MI_01#8&1f2e3d4c&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
        ));
        // An unreadable link must not silence a real device change.
        assert!(is_razer_interface(""));
    }

    /// A spawner that records every timer it is asked to start and never runs
    /// it, so the test controls when (if ever) the debouncer would fire.
    fn recording_spawner(fail: bool) -> (TimerSpawner, Arc<Mutex<usize>>) {
        let spawns = Arc::new(Mutex::new(0usize));
        let counter = Arc::clone(&spawns);
        let spawner: TimerSpawner = Box::new(move |_timer| {
            *counter.lock().unwrap() += 1;
            if fail {
                Err(io::Error::other("no threads left"))
            } else {
                Ok(())
            }
        });
        (spawner, spawns)
    }

    #[test]
    fn a_failed_timer_spawn_leaves_the_debouncer_ready_for_the_next_event() {
        let (spawner, spawns) = recording_spawner(true);
        let state = Arc::new(CallbackState::with_spawner(|| {}, spawner));

        // The thread could not be started: the burst must not be left armed,
        // otherwise no later device change would ever reach the worker.
        state.on_event();
        assert_eq!(*spawns.lock().unwrap(), 1);
        state.on_event();
        assert_eq!(
            *spawns.lock().unwrap(),
            2,
            "the next event must try to start a timer again"
        );
    }

    #[test]
    fn a_burst_starts_exactly_one_timer_thread() {
        let (spawner, spawns) = recording_spawner(false);
        let state = Arc::new(CallbackState::with_spawner(|| {}, spawner));

        state.on_event();
        state.on_event();
        state.on_event();
        assert_eq!(*spawns.lock().unwrap(), 1);
    }

    #[test]
    fn registration_succeeds_without_hardware_and_unregisters_cleanly() {
        // Runs on any Windows machine: cfgmgr32 accepts a HID interface filter
        // regardless of which devices are attached.
        let notifications =
            DeviceChangeNotifications::register(|| {}).expect("cfgmgr32 registration");
        drop(notifications);
    }
}
