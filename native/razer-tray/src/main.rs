#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
#[cfg(windows)]
mod device_events;
mod polling;
#[cfg(windows)]
mod power_events;
mod primary_device_menu;
mod resume_recovery;
mod settings_menu;
mod startup;

use std::error::Error;
use std::time::Duration;

use api::SharedIntegrationState;
#[cfg(windows)]
use device_events::DeviceChangeNotifications;
use notify_rust::{Notification, Urgency};
use polling::{CoreUpdate, CoreWorker};
#[cfg(windows)]
use power_events::PowerNotifications;
use primary_device_menu::{PrimaryDeviceMenu, canonical_preference};
use razer_core::frontend::FrontendSnapshot;
#[cfg(test)]
use razer_core::frontend::PollStatistics;
use razer_core::notifications::{NotificationKind, NotificationRequest};
use razer_core::settings::{AppSettings, PrimaryDevicePreference, SettingsStore};
use razer_core::state::DeviceReading;
use razer_core::transport::{BatteryTransport, TransportError};
#[cfg(windows)]
use razer_core::windows_hid::WindowsHidTransport;
use resume_recovery::{RecoveryOutcome, RecoveryTick, ResumeRecovery};
use settings_menu::{MIN_POLL_INTERVAL_SECONDS, SettingsMenu};
use single_instance::SingleInstance;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

const INSTANCE_NAME: &str = "RazerBatteryTaskbar.Native.Tray";

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Core(CoreUpdate),
    /// Windows resumed; the payload is the recovery generation for this wake.
    PowerResume(u64),
    /// A scheduled post-resume recovery refresh is due.
    RecoveryTick(RecoveryTick),
    /// A debounced USB/HID device arrival or removal asks for a rescan.
    DeviceChange,
}

struct PlaceholderTransport;

impl BatteryTransport for PlaceholderTransport {
    fn name(&self) -> &'static str {
        "placeholder"
    }

    fn is_ready(&self) -> bool {
        false
    }

    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
        Ok(Vec::new())
    }
}

enum AppTransport {
    Placeholder(PlaceholderTransport),
    #[cfg(windows)]
    WindowsHid(WindowsHidTransport),
}

impl BatteryTransport for AppTransport {
    fn name(&self) -> &'static str {
        match self {
            Self::Placeholder(transport) => transport.name(),
            #[cfg(windows)]
            Self::WindowsHid(transport) => transport.name(),
        }
    }

    fn is_ready(&self) -> bool {
        match self {
            Self::Placeholder(transport) => transport.is_ready(),
            #[cfg(windows)]
            Self::WindowsHid(transport) => transport.is_ready(),
        }
    }

    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
        match self {
            Self::Placeholder(transport) => transport.read_devices(),
            #[cfg(windows)]
            Self::WindowsHid(transport) => transport.read_devices(),
        }
    }
}

/// Which battery transport the tray starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportChoice {
    /// Native Windows HID feature reports (ADR-0001). The default.
    WindowsHid,
    /// Reads nothing and reports `ready=false`; debug/test runs without hardware only.
    Placeholder,
}

impl TransportChoice {
    fn into_transport(self) -> Result<AppTransport, Box<dyn Error>> {
        match self {
            Self::Placeholder => Ok(AppTransport::Placeholder(PlaceholderTransport)),
            #[cfg(windows)]
            Self::WindowsHid => Ok(AppTransport::WindowsHid(WindowsHidTransport::new())),
            #[cfg(not(windows))]
            Self::WindowsHid => Err("the Windows HID transport requires Windows".into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaunchOptions {
    Run(TransportChoice),
    Help,
}

/// Retired opt-in flag from the preview builds; accepted as a no-op.
const LEGACY_HID_FLAG: &str = "--experimental-hid-00b7";
const PLACEHOLDER_FLAG: &str = "--placeholder-transport";

fn parse_launch_options(args: &[String]) -> Result<LaunchOptions, String> {
    match args {
        [] => Ok(LaunchOptions::Run(TransportChoice::WindowsHid)),
        [option] if option == LEGACY_HID_FLAG => {
            eprintln!(
                "{LEGACY_HID_FLAG} is a no-op: the Windows HID transport is already the default."
            );
            Ok(LaunchOptions::Run(TransportChoice::WindowsHid))
        }
        [option] if option == PLACEHOLDER_FLAG => {
            Ok(LaunchOptions::Run(TransportChoice::Placeholder))
        }
        [option] if option == "--help" || option == "-h" => Ok(LaunchOptions::Help),
        _ => Err("invalid arguments: use --help to see supported options".to_string()),
    }
}

fn print_help() {
    println!("RazerBatteryTaskbar Native Preview");
    println!("Usage: razer-tray.exe [{PLACEHOLDER_FLAG}]");
    println!("Starts with the native Windows HID battery transport by default.");
    println!("  {PLACEHOLDER_FLAG}   debug only: run without any battery transport");
    println!("  {LEGACY_HID_FLAG}  retired; accepted as a no-op");
}

/// Resolve a saved primary-device preference to the canonical product ID the
/// core reports (the wired twin `00B6` becomes `00B7`). Returns `true` when
/// the settings were changed.
fn canonicalize_primary_device(settings: &mut AppSettings) -> bool {
    let canonical = canonical_preference(&settings.primary_device);
    if canonical == settings.primary_device {
        return false;
    }

    settings.primary_device = canonical;
    true
}

/// Raise a saved poll interval to the Windows HID transport's minimum.
/// Returns `true` when the settings were changed.
fn clamp_poll_interval(settings: &mut AppSettings) -> bool {
    if settings.poll_interval_seconds < MIN_POLL_INTERVAL_SECONDS {
        settings.poll_interval_seconds = MIN_POLL_INTERVAL_SECONDS;
        return true;
    }

    false
}

struct TrayApplication {
    worker: CoreWorker,
    settings_store: Option<SettingsStore>,
    settings: AppSettings,
    integration_state: SharedIntegrationState,
    last_snapshot: Option<FrontendSnapshot>,
    resume_recovery: ResumeRecovery,
    tray: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    primary_device_menu: Option<PrimaryDeviceMenu>,
    settings_menu: Option<SettingsMenu>,
    refresh_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
}

impl TrayApplication {
    fn new(
        settings_store: Option<SettingsStore>,
        settings: AppSettings,
        integration_state: SharedIntegrationState,
        worker: CoreWorker,
    ) -> Self {
        Self {
            worker,
            settings_store,
            settings,
            integration_state,
            last_snapshot: None,
            resume_recovery: ResumeRecovery::default(),
            tray: None,
            status_item: None,
            primary_device_menu: None,
            settings_menu: None,
            refresh_item: None,
            quit_item: None,
        }
    }

    fn create_tray(&mut self) -> Result<(), Box<dyn Error>> {
        let menu = Menu::new();
        let status_item = MenuItem::new("Starting native core…", false, None);
        let primary_device_menu = PrimaryDeviceMenu::new(&self.settings.primary_device)?;
        let settings_menu = SettingsMenu::new(&self.settings)?;
        let refresh_item = MenuItem::with_id("refresh", "Refresh", true, None);
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        menu.append(&status_item)?;
        menu.append(&primary_device_menu.root)?;
        menu.append(&settings_menu.root)?;
        menu.append(&refresh_item)?;
        menu.append(&quit_item)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(build_battery_icon(None, false)?)
            .with_tooltip("RazerBatteryTaskbar Native")
            .build()?;

        self.tray = Some(tray);
        self.status_item = Some(status_item);
        self.primary_device_menu = Some(primary_device_menu);
        self.settings_menu = Some(settings_menu);
        self.refresh_item = Some(refresh_item);
        self.quit_item = Some(quit_item);

        if let Some(snapshot) = &self.last_snapshot {
            self.render(snapshot);
        }
        self.sync_primary_device_menu();

        Ok(())
    }

    fn refresh(&self) {
        if !self.worker.request_refresh() {
            eprintln!("battery worker is unavailable");
        }
    }

    fn apply_core_update(&mut self, update: CoreUpdate) {
        api::update_state(&self.integration_state, &update.snapshot);

        if self.settings.notifications.enabled {
            for notification in &update.notifications {
                deliver_notification(notification);
            }
        }

        self.render(&update.snapshot);
        self.last_snapshot = Some(update.snapshot);
        self.sync_primary_device_menu();
    }

    fn render(&self, snapshot: &FrontendSnapshot) {
        let status = format_status(snapshot);

        if let Some(item) = &self.status_item {
            item.set_text(&status);
        }

        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some(&status));
            let (battery, charging) = if snapshot.transport_healthy && snapshot.transport_ready {
                let device = snapshot.primary_device.as_ref();
                (
                    device.and_then(|device| device.battery),
                    device.and_then(|device| device.charging) == Some(true),
                )
            } else {
                (None, false)
            };
            let _ = tray.set_icon(Some(
                build_battery_icon(battery, charging).expect("generated tray icon must be valid"),
            ));
        }
    }

    fn persist_settings(&self, context: &str) {
        if let Some(store) = &self.settings_store
            && let Err(error) = store.save(&self.settings)
        {
            eprintln!("failed to persist {context}: {error}");
        }
    }

    fn sync_settings_menu(&self) {
        if let Some(menu) = &self.settings_menu {
            menu.sync(&self.settings);
        }
    }

    fn sync_primary_device_menu(&mut self) {
        if let Some(menu) = &mut self.primary_device_menu {
            menu.update(self.last_snapshot.as_ref(), &self.settings.primary_device);
        }
    }

    /// The core answers with a snapshot for the new primary device; the icon,
    /// tooltip and `/v1/status` all follow from that same snapshot.
    fn set_primary_device(&mut self, preference: PrimaryDevicePreference) {
        self.settings.primary_device = preference.clone();
        if !self.worker.set_primary_device(preference) {
            eprintln!("battery worker could not update the primary device");
        }
        self.sync_primary_device_menu();
        self.persist_settings("primary device");
    }

    fn sync_worker_notifications(&self) {
        if !self
            .worker
            .set_notifications(self.settings.notifications.clone())
        {
            eprintln!("battery worker could not update notification preferences");
        }
    }

    fn toggle_notifications(&mut self) {
        self.settings.notifications.enabled = !self.settings.notifications.enabled;
        self.sync_worker_notifications();
        self.sync_settings_menu();
        self.persist_settings("notification setting");
    }

    fn set_poll_interval(&mut self, seconds: u64) {
        if seconds < MIN_POLL_INTERVAL_SECONDS {
            return;
        }

        self.settings.poll_interval_seconds = seconds;
        self.sync_settings_menu();
        self.persist_settings("poll interval");

        if !self.worker.set_interval(Duration::from_secs(seconds)) {
            eprintln!("failed to update active poll interval");
        }
    }

    fn set_low_battery_threshold(&mut self, percent: u8) {
        self.settings.notifications.low_battery_percent = percent;

        if self.settings.notifications.critical_battery_percent > percent {
            self.settings.notifications.critical_battery_percent = percent;
        }

        self.sync_worker_notifications();
        self.sync_settings_menu();
        self.persist_settings("low battery threshold");
    }

    fn set_critical_battery_threshold(&mut self, percent: u8) {
        if percent > self.settings.notifications.low_battery_percent {
            return;
        }

        self.settings.notifications.critical_battery_percent = percent;
        self.sync_worker_notifications();
        self.sync_settings_menu();
        self.persist_settings("critical battery threshold");
    }

    fn toggle_start_with_windows(&mut self) {
        let next = !self.settings.start_with_windows;

        match startup::set_enabled(next) {
            Ok(()) => {
                self.settings.start_with_windows = next;
                self.sync_settings_menu();
                self.persist_settings("start-with-Windows setting");
            }
            Err(error) => {
                eprintln!("failed to update start-with-Windows registration: {error}");
                self.sync_settings_menu();
            }
        }
    }

    fn handle_menu(&mut self, event_loop: &ActiveEventLoop, event: MenuEvent) {
        let primary_device = self
            .primary_device_menu
            .as_ref()
            .and_then(|menu| menu.preference_for_event(&event));
        if let Some(preference) = primary_device {
            self.set_primary_device(preference);
            return;
        }

        let notifications_event = self
            .settings_menu
            .as_ref()
            .is_some_and(|menu| menu.is_notifications_event(&event));
        if notifications_event {
            self.toggle_notifications();
            return;
        }

        let test_notification_event = self
            .settings_menu
            .as_ref()
            .is_some_and(|menu| menu.is_test_notification_event(&event));
        if test_notification_event {
            deliver_test_notification();
            return;
        }

        let start_with_windows_event = self
            .settings_menu
            .as_ref()
            .is_some_and(|menu| menu.is_start_with_windows_event(&event));
        if start_with_windows_event {
            self.toggle_start_with_windows();
            return;
        }

        let poll_interval = self
            .settings_menu
            .as_ref()
            .and_then(|menu| menu.poll_interval_for_event(&event));
        if let Some(seconds) = poll_interval {
            self.set_poll_interval(seconds);
            return;
        }

        let low_battery = self
            .settings_menu
            .as_ref()
            .and_then(|menu| menu.low_battery_for_event(&event));
        if let Some(percent) = low_battery {
            self.set_low_battery_threshold(percent);
            return;
        }

        let critical_battery = self
            .settings_menu
            .as_ref()
            .and_then(|menu| menu.critical_battery_for_event(&event));
        if let Some(percent) = critical_battery {
            self.set_critical_battery_threshold(percent);
            return;
        }

        if self
            .refresh_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            self.refresh();
            return;
        }

        if self
            .quit_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<UserEvent> for TrayApplication {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            self.create_tray()
                .expect("native Windows tray should initialize");
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(event) => self.handle_menu(event_loop, event),
            UserEvent::Core(update) => {
                if self.resume_recovery.on_update(&update.snapshot) == RecoveryOutcome::Exhausted {
                    eprintln!("post-resume recovery refreshes failed; regular polling continues");
                }
                self.apply_core_update(update);
            }
            UserEvent::PowerResume(generation) => self.resume_recovery.on_resume(generation),
            UserEvent::RecoveryTick(tick) => {
                if self.resume_recovery.on_tick(tick) {
                    self.refresh();
                }
            }
            UserEvent::DeviceChange => self.refresh(),
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

fn load_settings() -> (Option<SettingsStore>, AppSettings) {
    let store = match SettingsStore::for_current_user() {
        Ok(store) => store,
        Err(error) => {
            eprintln!("settings persistence unavailable; using defaults: {error}");
            return (None, AppSettings::default());
        }
    };

    match store.load() {
        Ok(settings) => (Some(store), settings),
        Err(error) => {
            eprintln!("failed to load settings; using defaults: {error}");
            (Some(store), AppSettings::default())
        }
    }
}

fn reconcile_startup_setting(store: Option<&SettingsStore>, settings: &mut AppSettings) {
    match startup::is_enabled() {
        Ok(enabled) if enabled != settings.start_with_windows => {
            settings.start_with_windows = enabled;

            if let Some(store) = store
                && let Err(error) = store.save(settings)
            {
                eprintln!("failed to reconcile start-with-Windows setting: {error}");
            }
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!("failed to read start-with-Windows registration: {error}");
        }
    }
}

fn deliver_notification(request: &NotificationRequest) {
    let urgency = match request.kind {
        NotificationKind::LowBattery => Urgency::Normal,
        NotificationKind::CriticalBattery => Urgency::Critical,
    };

    if let Err(error) = Notification::new()
        .summary(&request.title)
        .body(&request.body)
        .urgency(urgency)
        .show()
    {
        eprintln!("failed to show Windows battery notification: {error}");
    }
}

fn deliver_test_notification() {
    if let Err(error) = Notification::new()
        .summary("RazerBatteryTaskbar")
        .body("Native Windows notification test.")
        .urgency(Urgency::Normal)
        .show()
    {
        eprintln!("failed to show Windows test notification: {error}");
    }
}

fn format_status(snapshot: &FrontendSnapshot) -> String {
    if !snapshot.transport_ready {
        return "Placeholder transport (debug) — no battery data".to_string();
    }

    if !snapshot.transport_healthy {
        return format!(
            "HID battery read failed ({} consecutive failures; {})",
            snapshot.consecutive_transport_failures,
            snapshot
                .transport_error_kind
                .as_deref()
                .unwrap_or("unknown")
        );
    }

    match snapshot.primary_device.as_ref() {
        Some(device) if device.connected => {
            let name = device.name.as_deref().unwrap_or("Razer device");
            let charging = if device.charging == Some(true) {
                " — Charging"
            } else {
                ""
            };
            match device.battery {
                Some(battery) => format!("{name} — {battery:.1}%{charging}"),
                None => format!("{name} — Battery unavailable{charging}"),
            }
        }
        _ => "No supported Razer devices detected".to_string(),
    }
}

fn build_battery_icon(battery: Option<f32>, charging: bool) -> Result<Icon, tray_icon::BadIcon> {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;

    Icon::from_rgba(battery_icon_rgba(battery, charging), WIDTH, HEIGHT)
}

/// Lightning bolt drawn inside the battery cell while charging, as
/// (x, y) pixels. It spans the interior rows 5..=10 and columns 5..=8.
const CHARGING_BOLT_PIXELS: [(u32, u32); 14] = [
    (7, 5),
    (8, 5),
    (6, 6),
    (7, 6),
    (5, 7),
    (6, 7),
    (7, 7),
    (8, 7),
    (7, 8),
    (8, 8),
    (6, 9),
    (7, 9),
    (5, 10),
    (6, 10),
];

fn battery_icon_rgba(battery: Option<f32>, charging: bool) -> Vec<u8> {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;

    let mut rgba = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let fill_width = battery.map(|level| {
        let normalized = level.clamp(0.0, 100.0) / 100.0;
        (normalized * 9.0).round() as u32
    });

    let mut set_pixel = |x: u32, y: u32, value: u8| {
        let index = ((y * WIDTH + x) * 4) as usize;
        rgba[index] = value;
        rgba[index + 1] = value;
        rgba[index + 2] = value;
        rgba[index + 3] = 255;
    };

    for x in 2..13 {
        set_pixel(x, 4, 220);
        set_pixel(x, 11, 220);
    }

    for y in 4..=11 {
        set_pixel(2, y, 220);
        set_pixel(12, y, 220);
    }

    for y in 6..=9 {
        set_pixel(13, y, 220);
    }

    match fill_width {
        Some(fill_width) => {
            for x in 3..(3 + fill_width) {
                for y in 5..11 {
                    set_pixel(x, y, 255);
                }
            }
        }
        // While charging the bolt replaces the unknown marker; a dimmed bolt
        // in an empty cell is the charging-with-unknown-battery state.
        None if charging => {}
        None => {
            // Unknown is not zero: draw a small question mark inside the empty cell.
            for (x, y) in [
                (5, 6),
                (6, 5),
                (7, 5),
                (8, 5),
                (9, 6),
                (9, 7),
                (8, 8),
                (7, 10),
            ] {
                set_pixel(x, y, 255);
            }
        }
    }

    if charging {
        // Invert the bolt against whatever is underneath so it stays legible
        // over the white fill as well as over the empty (transparent) cell.
        let unfilled_value = if battery.is_some() { 255 } else { 200 };
        for (x, y) in CHARGING_BOLT_PIXELS {
            let filled = fill_width.is_some_and(|fill_width| x < 3 + fill_width);
            set_pixel(x, y, if filled { 0 } else { unfilled_value });
        }
    }

    rgba
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let transport_choice = match parse_launch_options(&args)? {
        LaunchOptions::Run(choice) => choice,
        LaunchOptions::Help => {
            print_help();
            return Ok(());
        }
    };

    let instance_guard = SingleInstance::new(INSTANCE_NAME)?;

    if !instance_guard.is_single() {
        return Ok(());
    }

    let (settings_store, mut settings) = load_settings();
    reconcile_startup_setting(settings_store.as_ref(), &mut settings);

    let clamped = clamp_poll_interval(&mut settings);
    let canonicalized = canonicalize_primary_device(&mut settings);
    if (clamped || canonicalized)
        && let Some(store) = settings_store.as_ref()
        && let Err(error) = store.save(&settings)
    {
        eprintln!("failed to persist normalized settings: {error}");
    }

    let transport = transport_choice.into_transport()?;

    let initial_poll_interval = Duration::from_secs(settings.poll_interval_seconds);

    let integration_state = api::initial_state(transport.name(), transport.is_ready());
    let _api_thread = match api::start(integration_state.clone()) {
        Ok(thread) => Some(thread),
        Err(error) => {
            eprintln!(
                "integration API unavailable on http://{}{}: {error}",
                api::API_ADDRESS,
                api::STATUS_PATH
            );
            None
        }
    };

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;

    // Windows sends an automatic resume event, often followed by a second
    // user-resume event. The power callback deduplicates the pair. The
    // receiver needs several seconds to re-pair with the mouse, so recovery
    // refreshes are staggered (see `resume_recovery`) and stop at the first
    // real battery read.
    #[cfg(windows)]
    let _power_notifications = {
        let resume_proxy = event_loop.create_proxy();
        match PowerNotifications::register(move || {
            let generation = resume_recovery::next_generation();
            let _ = resume_proxy.send_event(UserEvent::PowerResume(generation));

            let tick_proxy = resume_proxy.clone();
            resume_recovery::spawn_ticks(generation, move |tick| {
                let _ = tick_proxy.send_event(UserEvent::RecoveryTick(tick));
            });
        }) {
            Ok(registration) => Some(registration),
            Err(error) => {
                eprintln!("Windows resume notification unavailable: {error}");
                None
            }
        }
    };

    // Unplugging or replugging the receiver produces a burst of HID interface
    // notifications. The device-change module debounces them into rescans.
    #[cfg(windows)]
    let _device_notifications = {
        let rescan_proxy = event_loop.create_proxy();
        match DeviceChangeNotifications::register(move || {
            let _ = rescan_proxy.send_event(UserEvent::DeviceChange);
        }) {
            Ok(registration) => Some(registration),
            Err(error) => {
                eprintln!("Windows device-change notification unavailable: {error}");
                None
            }
        }
    };

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let update_proxy = event_loop.create_proxy();
    let worker = CoreWorker::start(
        transport,
        initial_poll_interval,
        settings.notifications.clone(),
        settings.primary_device.clone(),
        move |update| update_proxy.send_event(UserEvent::Core(update)).is_ok(),
    );

    let mut application = TrayApplication::new(settings_store, settings, integration_state, worker);
    event_loop.run_app(&mut application)?;
    drop(instance_guard);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::frontend::FrontendDeviceState;
    use razer_core::state::DeviceType;

    fn healthy_snapshot(battery: Option<f32>, charging: Option<bool>) -> FrontendSnapshot {
        let device = FrontendDeviceState {
            vendor_id: 0x1532,
            product_id: 0x00B7,
            name: Some("Razer DeathAdder V3 Pro".to_string()),
            device_type: DeviceType::Mouse,
            battery,
            charging,
            connected: true,
        };
        FrontendSnapshot {
            devices: vec![device.clone()],
            primary_device: Some(device),
            transport_name: "windows-hid".to_string(),
            transport_ready: true,
            transport_healthy: true,
            consecutive_transport_failures: 0,
            transport_error_kind: None,
            poll_statistics: PollStatistics::default(),
        }
    }

    #[test]
    fn status_text_appends_charging_in_the_existing_format() {
        assert_eq!(
            format_status(&healthy_snapshot(Some(64.0), Some(true))),
            "Razer DeathAdder V3 Pro — 64.0% — Charging"
        );
        assert_eq!(
            format_status(&healthy_snapshot(None, Some(true))),
            "Razer DeathAdder V3 Pro — Battery unavailable — Charging"
        );
        assert_eq!(
            format_status(&healthy_snapshot(Some(64.0), Some(false))),
            "Razer DeathAdder V3 Pro — 64.0%"
        );
        assert_eq!(
            format_status(&healthy_snapshot(Some(64.0), None)),
            "Razer DeathAdder V3 Pro — 64.0%"
        );
    }

    #[test]
    fn charging_icon_differs_from_non_charging_icon_at_every_battery_level() {
        for level in [0.0, 37.0, 50.0, 100.0] {
            assert!(build_battery_icon(Some(level), true).is_ok());
            assert_ne!(
                battery_icon_rgba(Some(level), true),
                battery_icon_rgba(Some(level), false),
                "charging mark missing at {level}%"
            );
        }
        // Not charging and unknown charging render the same icon as before.
        assert_eq!(
            battery_icon_rgba(Some(50.0), false).len(),
            (16 * 16 * 4) as usize
        );
    }

    #[test]
    fn charging_icon_renders_with_unknown_battery() {
        assert!(build_battery_icon(None, true).is_ok());
        assert_ne!(
            battery_icon_rgba(None, true),
            battery_icon_rgba(None, false)
        );
        assert_ne!(
            battery_icon_rgba(None, true),
            battery_icon_rgba(Some(0.0), true)
        );
    }

    #[test]
    fn placeholder_status_is_explicit_that_no_battery_transport_is_active() {
        let snapshot = FrontendSnapshot {
            devices: Vec::new(),
            primary_device: None,
            transport_name: "placeholder".to_string(),
            transport_ready: false,
            transport_healthy: true,
            consecutive_transport_failures: 0,
            transport_error_kind: None,
            poll_statistics: PollStatistics::default(),
        };

        assert_eq!(
            format_status(&snapshot),
            "Placeholder transport (debug) — no battery data"
        );
    }

    #[test]
    fn failed_transport_state_does_not_display_stale_battery() {
        let snapshot = FrontendSnapshot {
            devices: Vec::new(),
            primary_device: None,
            transport_name: "windows-hid".to_string(),
            transport_ready: true,
            transport_healthy: false,
            consecutive_transport_failures: 2,
            transport_error_kind: None,
            poll_statistics: PollStatistics::default(),
        };

        assert!(format_status(&snapshot).contains("2 consecutive failures"));
        assert!(format_status(&snapshot).contains("unknown"));
    }

    #[test]
    fn default_launch_selects_the_windows_hid_transport() {
        let options = parse_launch_options(&[]).unwrap();

        assert_eq!(options, LaunchOptions::Run(TransportChoice::WindowsHid));
    }

    #[test]
    fn retired_hid_flag_is_accepted_as_a_no_op() {
        let options = parse_launch_options(&["--experimental-hid-00b7".to_string()]).unwrap();

        assert_eq!(options, LaunchOptions::Run(TransportChoice::WindowsHid));
    }

    #[test]
    fn placeholder_transport_needs_the_explicit_debug_flag() {
        let options = parse_launch_options(&["--placeholder-transport".to_string()]).unwrap();
        assert_eq!(options, LaunchOptions::Run(TransportChoice::Placeholder));

        assert_eq!(
            parse_launch_options(&["--help".to_string()]).unwrap(),
            LaunchOptions::Help
        );
        assert!(parse_launch_options(&["--placeholder".to_string()]).is_err());
        assert!(
            parse_launch_options(&[
                "--placeholder-transport".to_string(),
                "--experimental-hid-00b7".to_string()
            ])
            .is_err()
        );
    }

    #[cfg(windows)]
    #[test]
    fn default_transport_is_the_ready_windows_hid_implementation() {
        let transport = TransportChoice::WindowsHid.into_transport().unwrap();

        assert_eq!(transport.name(), "windows-hid");
        assert!(transport.is_ready());
    }

    #[test]
    fn placeholder_transport_is_explicitly_not_ready() {
        let transport = TransportChoice::Placeholder.into_transport().unwrap();

        assert_eq!(transport.name(), "placeholder");
        assert!(!transport.is_ready());
    }

    #[test]
    fn saved_poll_intervals_below_the_transport_minimum_are_clamped() {
        let mut settings = AppSettings {
            poll_interval_seconds: 5,
            ..AppSettings::default()
        };
        assert!(clamp_poll_interval(&mut settings));
        assert_eq!(settings.poll_interval_seconds, 60);

        let mut settings = AppSettings {
            poll_interval_seconds: 300,
            ..AppSettings::default()
        };
        assert!(!clamp_poll_interval(&mut settings));
        assert_eq!(settings.poll_interval_seconds, 300);
    }

    #[test]
    fn saved_wired_twin_preference_is_rewritten_to_the_canonical_product_id() {
        let mut settings = AppSettings {
            primary_device: PrimaryDevicePreference::ProductId(0x00B6),
            ..AppSettings::default()
        };
        assert!(canonicalize_primary_device(&mut settings));
        assert_eq!(
            settings.primary_device,
            PrimaryDevicePreference::ProductId(0x00B7)
        );

        assert!(!canonicalize_primary_device(&mut settings));
        let mut automatic = AppSettings::default();
        assert!(!canonicalize_primary_device(&mut automatic));
    }

    #[test]
    fn generated_icons_are_valid_at_extreme_battery_levels() {
        assert!(build_battery_icon(Some(0.0), false).is_ok());
        assert!(build_battery_icon(Some(100.0), false).is_ok());
        assert!(build_battery_icon(None, false).is_ok());
    }

    #[test]
    fn unknown_battery_icon_is_distinct_from_empty_battery() {
        assert_ne!(
            battery_icon_rgba(None, false),
            battery_icon_rgba(Some(0.0), false)
        );
        assert_ne!(
            battery_icon_rgba(None, false),
            battery_icon_rgba(Some(100.0), false)
        );
        assert_eq!(
            battery_icon_rgba(Some(0.0), false),
            battery_icon_rgba(Some(-10.0), false)
        );
        assert_eq!(
            battery_icon_rgba(Some(100.0), false),
            battery_icon_rgba(Some(110.0), false)
        );
    }
}
