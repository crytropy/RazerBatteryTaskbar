use std::error::Error;
use std::sync::{Arc, RwLock};
use std::thread;

use razer_core::frontend::{FrontendSnapshot, PollStatistics};
use razer_core::integration::IntegrationSnapshot;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub const API_ADDRESS: &str = "127.0.0.1:27212";
pub const STATUS_PATH: &str = "/v1/status";
/// Same JSON body as `STATUS_PATH`, served as `text/plain`.
///
/// Seelen UI (2.8.x) parses `application/json` remote data into a Svelte deep
/// state proxy, and any access to that proxy from a sandboxed plugin script
/// throws `state_unsafe_mutation`, so JSON remote data is unusable there. A
/// plain-text response stays a primitive string, which the plugin can
/// `JSON.parse` itself. See `integrations/seelen-ui/README.md`.
pub const STATUS_TEXT_PATH: &str = "/v1/status.txt";
const JSON_CONTENT_TYPE: &str = "application/json; charset=UTF-8";
const TEXT_CONTENT_TYPE: &str = "text/plain; charset=UTF-8";
const SEELEN_ORIGIN: &str = "http://tauri.localhost";

/// Content type for a status URL, or `None` when the path is not served.
pub fn status_content_type(url: &str) -> Option<&'static str> {
    match url {
        STATUS_PATH => Some(JSON_CONTENT_TYPE),
        STATUS_TEXT_PATH => Some(TEXT_CONTENT_TYPE),
        _ => None,
    }
}

pub type SharedIntegrationState = Arc<RwLock<String>>;

/// Snapshot served until the worker completes its first read: the selected
/// transport with no devices and zeroed poll statistics.
pub fn initial_state(transport_name: &str, transport_ready: bool) -> SharedIntegrationState {
    let snapshot = FrontendSnapshot {
        devices: Vec::new(),
        primary_device: None,
        transport_name: transport_name.to_string(),
        transport_ready,
        transport_healthy: true,
        consecutive_transport_failures: 0,
        transport_error_kind: None,
        poll_statistics: PollStatistics::default(),
    };

    Arc::new(RwLock::new(
        IntegrationSnapshot::from_frontend(&snapshot)
            .to_json()
            .expect("empty integration snapshot must serialize"),
    ))
}

pub fn update_state(state: &SharedIntegrationState, snapshot: &FrontendSnapshot) {
    let json = match IntegrationSnapshot::from_frontend(snapshot).to_json() {
        Ok(json) => json,
        Err(error) => {
            eprintln!("failed to serialize integration snapshot: {error}");
            return;
        }
    };

    match state.write() {
        Ok(mut current) => *current = json,
        Err(error) => eprintln!("integration snapshot lock poisoned: {error}"),
    }
}

pub fn start(
    state: SharedIntegrationState,
) -> Result<thread::JoinHandle<()>, Box<dyn Error + Send + Sync>> {
    let server = Server::http(API_ADDRESS)?;

    Ok(thread::spawn(move || {
        for request in server.incoming_requests() {
            respond(request, &state);
        }
    }))
}

fn respond(request: Request, state: &SharedIntegrationState) {
    if request.method() != &Method::Get {
        send_response(
            request,
            StatusCode(405),
            "Method Not Allowed",
            "text/plain; charset=UTF-8",
        );
        return;
    }

    let Some(content_type) = status_content_type(request.url()) else {
        send_response(
            request,
            StatusCode(404),
            "Not Found",
            "text/plain; charset=UTF-8",
        );
        return;
    };

    let body = match state.read() {
        Ok(current) => current.clone(),
        Err(error) => {
            eprintln!("integration snapshot read lock poisoned: {error}");
            send_response(
                request,
                StatusCode(503),
                r#"{"error":"integration state unavailable"}"#,
                content_type,
            );
            return;
        }
    };

    send_response(request, StatusCode(200), body, content_type);
}

fn send_response(
    request: Request,
    status: StatusCode,
    body: impl Into<String>,
    content_type: &'static str,
) {
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("Access-Control-Allow-Origin", SEELEN_ORIGIN));

    if let Err(error) = request.respond(response) {
        eprintln!("failed to send integration API response: {error}");
    }
}

fn header(name: &'static str, value: &'static str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .expect("static HTTP header must contain valid ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::frontend::FrontendDeviceState;
    use razer_core::state::DeviceType;

    #[test]
    fn initial_api_state_is_valid_versioned_json() {
        let state = initial_state("windows-hid", true);
        let json = state.read().unwrap();

        assert!(json.contains(r#""schemaVersion":1"#));
        assert!(json.contains(r#""transportName":"windows-hid""#));
        assert!(json.contains(r#""transportReady":true"#));
        assert!(json.contains(r#""devices":[]"#));
        assert!(json.contains(r#""attempts":0"#));
    }

    #[test]
    fn initial_api_state_reflects_the_placeholder_debug_transport() {
        let state = initial_state("placeholder", false);
        let json = state.read().unwrap();

        assert!(json.contains(r#""transportName":"placeholder""#));
        assert!(json.contains(r#""transportReady":false"#));
    }

    #[test]
    fn text_status_path_serves_the_same_body_as_plain_text() {
        assert_eq!(
            status_content_type("/v1/status"),
            Some("application/json; charset=UTF-8")
        );
        assert_eq!(
            status_content_type("/v1/status.txt"),
            Some("text/plain; charset=UTF-8")
        );
        assert_eq!(status_content_type("/v1/status.json"), None);
        assert_eq!(status_content_type("/"), None);
    }

    #[test]
    fn update_state_publishes_latest_frontend_snapshot_without_private_ids() {
        let state = initial_state("windows-hid", true);
        let snapshot = FrontendSnapshot {
            devices: vec![FrontendDeviceState {
                vendor_id: 0x1532,
                product_id: 0x00AB,
                name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
                device_type: DeviceType::Mouse,
                battery: Some(72.5),
                charging: None,
                connected: true,
            }],
            primary_device: None,
            transport_name: "scripted".to_string(),
            transport_ready: true,
            transport_healthy: true,
            consecutive_transport_failures: 0,
            transport_error_kind: None,
            poll_statistics: PollStatistics::default(),
        };

        update_state(&state, &snapshot);

        let json = state.read().unwrap();
        assert!(json.contains(r#""battery":72.5"#));
        assert!(!json.contains("serial"));
        assert!(!json.contains("usb:1532"));
    }
}
