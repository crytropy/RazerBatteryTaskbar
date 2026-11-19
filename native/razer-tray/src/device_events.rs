//! Windows USB/HID device-change callbacks for the native tray.
//! No HID operations happen inside the Windows callback.
//!
//! `CM_Register_Notification` (cfgmgr32) needs no window: like the power
//! callbacks it runs on a system thread. The callback only filters for Razer
//! interfaces and forwards each relevant event; the worker's refresh schedule
//! turns a burst of them into one device-change rescan.
use std::ffi::c_void;
use std::io;
use std::sync::Arc;
use std::{ptr, slice};

use razer_core::RAZER_VENDOR_ID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
    CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER,
    CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CM_Register_Notification, CM_Unregister_Notification,
    CR_SUCCESS, HCMNOTIFICATION,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;

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

struct CallbackState {
    callback: Box<dyn Fn() + Send + Sync>,
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
        (state.callback)();
    }

    0
}

pub struct DeviceChangeNotifications {
    registration: HCMNOTIFICATION,
    context: *const CallbackState,
}

impl DeviceChangeNotifications {
    /// `callback` is invoked once per Razer interface arrival or removal, on
    /// a Windows thread-pool thread; it should only enqueue work for the worker.
    pub fn register(callback: impl Fn() + Send + Sync + 'static) -> io::Result<Self> {
        let context = Arc::into_raw(Arc::new(CallbackState {
            callback: Box::new(callback),
        }));

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
        // the context is released only after Windows stopped using it.
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

    #[test]
    fn registration_succeeds_without_hardware_and_unregisters_cleanly() {
        // Runs on any Windows machine: cfgmgr32 accepts a HID interface filter
        // regardless of which devices are attached.
        let notifications =
            DeviceChangeNotifications::register(|| {}).expect("cfgmgr32 registration");
        drop(notifications);
    }
}
