//! Windows suspend/resume callbacks for the native tray.
//! No HID operations happen inside the Windows callback.
use std::ffi::c_void;
use std::io;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Power::{
    DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, PowerRegisterSuspendResumeNotification,
    PowerUnregisterSuspendResumeNotification,
};

const DEVICE_NOTIFY_CALLBACK_FLAG: u32 = 2;
const PBT_APMRESUMEAUTOMATIC: u32 = 0x12;
const PBT_APMRESUMESUSPEND: u32 = 0x07;
const RESUME_DEBOUNCE: Duration = Duration::from_secs(4);

struct CallbackState {
    callback: Box<dyn Fn() + Send + Sync>,
    last_resume: Mutex<Option<Instant>>,
}

fn should_forward(event: u32, last_resume: &mut Option<Instant>, now: Instant) -> bool {
    if event != PBT_APMRESUMEAUTOMATIC && event != PBT_APMRESUMESUSPEND {
        return false;
    }

    if last_resume.is_some_and(|last| now.duration_since(last) < RESUME_DEBOUNCE) {
        return false;
    }

    *last_resume = Some(now);
    true
}

unsafe extern "system" fn resume_callback(
    context: *const c_void,
    event: u32,
    _setting: *const c_void,
) -> u32 {
    if context.is_null() {
        return 0;
    }

    // SAFETY: registration owns the boxed CallbackState; its memory is freed
    // only after Windows has unregistered the callback.
    let state = unsafe { &*context.cast::<CallbackState>() };

    let forward = match state.last_resume.lock() {
        Ok(mut last_resume) => should_forward(event, &mut last_resume, Instant::now()),
        Err(_) => false,
    };

    if forward {
        // The callback only enqueues work. The tray and HID worker run elsewhere.
        (state.callback)();
    }

    0
}

pub struct PowerNotifications {
    registration: *mut c_void,
    context: *mut CallbackState,
}

impl PowerNotifications {
    pub fn register(callback: impl Fn() + Send + Sync + 'static) -> io::Result<Self> {
        let context = Box::into_raw(Box::new(CallbackState {
            callback: Box::new(callback),
            last_resume: Mutex::new(None),
        }));
        let mut parameters = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(resume_callback),
            Context: context.cast(),
        };
        let mut registration = std::ptr::null_mut();

        // SAFETY: parameters and its callback context remain valid for the
        // registration call; CallbackState then lives until unregister.
        let result = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK_FLAG,
                (&mut parameters as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS).cast(),
                &mut registration,
            )
        };

        if result != 0 {
            // SAFETY: Windows did not register this context on failure.
            unsafe { drop(Box::from_raw(context)) };
            return Err(io::Error::from_raw_os_error(result as i32));
        }

        Ok(Self {
            registration,
            context,
        })
    }
}

impl Drop for PowerNotifications {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful registration. Unregister
        // before freeing the callback state so Windows cannot use freed memory.
        unsafe {
            let _ = PowerUnregisterSuspendResumeNotification(self.registration as isize);
            drop(Box::from_raw(self.context));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_automatic_and_user_resume_and_debounces_duplicates() {
        let mut last = None;
        let now = Instant::now();

        assert!(!should_forward(0x04, &mut last, now));
        assert_eq!(last, None);
        assert!(should_forward(PBT_APMRESUMEAUTOMATIC, &mut last, now));
        assert!(!should_forward(
            PBT_APMRESUMESUSPEND,
            &mut last,
            now + Duration::from_secs(1)
        ));
        assert!(should_forward(
            PBT_APMRESUMEAUTOMATIC,
            &mut last,
            now + Duration::from_secs(5)
        ));
    }
}
