#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod startup;

use std::error::Error;
use std::thread;
use std::time::Duration;

use api::SharedIntegrationState;
use notify_rust::{Notification, Urgency};
use razer_core::frontend::FrontendSnapshot;
use razer_core::notifications::{NotificationKind, NotificationRequest, evaluate_events};
use razer_core::service::CoreService;
use razer_core::settings::{AppSettings, SettingsStore};
use razer_core::state::DeviceReading;
use razer_core::transport::{BatteryTransport, TransportError};
use single_instance::SingleInstance;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

const INSTANCE_NAME: &str = "RazerBatteryTaskbar.Native.Tray";

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Tick,
}

struct PlaceholderTransport;

impl BatteryTransport for PlaceholderTransport {
    fn name(&self) -> &'static str {
        "placeholder"
    }

    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
        Ok(Vec::new())
    }
}

struct TrayApplication {
    core: CoreService<PlaceholderTransport>,
    settings_store: Option<SettingsStore>,
    settings: AppSettings,
    integration_state: SharedIntegrationState,
    tray: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    notifications_item: Option<CheckMenuItem>,
    start_with_windows_item: Option<CheckMenuItem>,
    refresh_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
}

impl TrayApplication {
    fn new(
        settings_store: Option<SettingsStore>,
        settings: AppSettings,
        integration_state: SharedIntegrationState,
    ) -> Self {
        Self {
            core: CoreService::new(PlaceholderTransport),
            settings_store,
            settings,
            integration_state,
            tray: None,
            status_item: None,
            notifications_item: None,
            start_with_windows_item: None,
            refresh_item: None,
            quit_item: None,
        }
    }

    fn create_tray(&mut self) -> Result<(), Box<dyn Error>> {
        let menu = Menu::new();
        let status_item = MenuItem::new("Starting native core…", false, None);
        let notifications_item = CheckMenuItem::with_id(
            "notifications",
            "Notifications",
            true,
            self.settings.notifications.enabled,
            None,
        );
        let start_with_windows_item = CheckMenuItem::with_id(
            "start-with-windows",
            "Start with Windows",
            true,
            self.settings.start_with_windows,
            None,
        );
        let refresh_item = MenuItem::with_id("refresh", "Refresh", true, None);
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        menu.append(&status_item)?;
        menu.append(&notifications_item)?;
        menu.append(&start_with_windows_item)?;
        menu.append(&refresh_item)?;
        menu.append(&quit_item)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(build_battery_icon(None)?)
            .with_tooltip("RazerBatteryTaskbar Native")
            .build()?;

        self.tray = Some(tray);
        self.status_item = Some(status_item);
        self.notifications_item = Some(notifications_item);
        self.start_with_windows_item = Some(start_with_windows_item);
        self.refresh_item = Some(refresh_item);
        self.quit_item = Some(quit_item);

        Ok(())
    }

    fn refresh(&mut self) {
        let result = self.core.refresh();
        let snapshot = self.core.frontend_snapshot();
        api::update_state(&self.integration_state, &snapshot);

        for notification in evaluate_events(&result.events, &self.settings.notifications) {
            deliver_notification(&notification);
        }

        self.render(&snapshot);
    }

    fn render(&self, snapshot: &FrontendSnapshot) {
        let status = format_status(snapshot);

        if let Some(item) = &self.status_item {
            item.set_text(&status);
        }

        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some(&status));
            let _ = tray.set_icon(Some(
                build_battery_icon(
                    snapshot
                        .primary_device
                        .as_ref()
                        .and_then(|device| device.battery),
                )
                .expect("generated tray icon must be valid"),
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

    fn toggle_notifications(&mut self) {
        self.settings.notifications.enabled = !self.settings.notifications.enabled;

        if let Some(item) = &self.notifications_item {
            item.set_checked(self.settings.notifications.enabled);
        }

        self.persist_settings("notification setting");
    }

    fn toggle_start_with_windows(&mut self) {
        let next = !self.settings.start_with_windows;

        match startup::set_enabled(next) {
            Ok(()) => {
                self.settings.start_with_windows = next;

                if let Some(item) = &self.start_with_windows_item {
                    item.set_checked(next);
                }

                self.persist_settings("start-with-Windows setting");
            }
            Err(error) => {
                eprintln!("failed to update start-with-Windows registration: {error}");

                if let Some(item) = &self.start_with_windows_item {
                    item.set_checked(self.settings.start_with_windows);
                }
            }
        }
    }

    fn handle_menu(&mut self, event_loop: &ActiveEventLoop, event: MenuEvent) {
        if self
            .notifications_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            self.toggle_notifications();
            return;
        }

        if self
            .start_with_windows_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            self.toggle_start_with_windows();
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
            self.refresh();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(event) => self.handle_menu(event_loop, event),
            UserEvent::Tick => self.refresh(),
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

fn format_status(snapshot: &FrontendSnapshot) -> String {
    match snapshot.primary_device.as_ref() {
        Some(device) if device.connected => match device.battery {
            Some(battery) => format!(
                "{} — {battery:.1}%",
                device.name.as_deref().unwrap_or("Razer device")
            ),
            None => format!(
                "{} — Battery unavailable",
                device.name.as_deref().unwrap_or("Razer device")
            ),
        },
        _ => "Native shell ready — HID transport pending".to_string(),
    }
}

fn build_battery_icon(battery: Option<f32>) -> Result<Icon, tray_icon::BadIcon> {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;

    let mut rgba = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let level = battery.unwrap_or(0.0).clamp(0.0, 100.0);
    let fill_width = ((level / 100.0) * 9.0).round() as u32;

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

    for x in 3..(3 + fill_width) {
        for y in 5..11 {
            set_pixel(x, y, 255);
        }
    }

    Icon::from_rgba(rgba, WIDTH, HEIGHT)
}

fn main() -> Result<(), Box<dyn Error>> {
    let instance_guard = SingleInstance::new(INSTANCE_NAME)?;

    if !instance_guard.is_single() {
        return Ok(());
    }

    let (settings_store, mut settings) = load_settings();
    reconcile_startup_setting(settings_store.as_ref(), &mut settings);
    let poll_interval = Duration::from_secs(settings.poll_interval_seconds);

    let integration_state = api::initial_state();
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

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let tick_proxy = event_loop.create_proxy();
    thread::spawn(move || {
        loop {
            thread::sleep(poll_interval);

            if tick_proxy.send_event(UserEvent::Tick).is_err() {
                break;
            }
        }
    });

    let mut application = TrayApplication::new(settings_store, settings, integration_state);
    event_loop.run_app(&mut application)?;
    drop(instance_guard);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_shell_status_is_explicit_about_pending_hid_transport() {
        let snapshot = FrontendSnapshot {
            devices: Vec::new(),
            primary_device: None,
            transport_healthy: true,
            consecutive_transport_failures: 0,
        };

        assert_eq!(
            format_status(&snapshot),
            "Native shell ready — HID transport pending"
        );
    }

    #[test]
    fn generated_icons_are_valid_at_extreme_battery_levels() {
        assert!(build_battery_icon(Some(0.0)).is_ok());
        assert!(build_battery_icon(Some(100.0)).is_ok());
        assert!(build_battery_icon(None).is_ok());
    }
}
