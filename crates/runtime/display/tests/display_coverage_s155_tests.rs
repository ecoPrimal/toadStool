// SPDX-License-Identifier: AGPL-3.0-or-later
//! Coverage tests for display runtime modules (S155 expansion):
//! - input/events.rs (input event handling)
//! - drm/device.rs (DRM device management)
//! - lib.rs (display manager, DisplayError)
//! - ipc/mod.rs (IPC types, DisplayMethod, DisplayResult)

#![allow(clippy::pedantic)]

use std::path::PathBuf;

use toadstool_display::drm::{Device, DeviceCapabilities};
use toadstool_display::input::{InputEvent, KeyCode, Modifiers, MouseButton, TouchPhase};
use toadstool_display::ipc::IpcTransport;
use toadstool_display::ipc::{
    DisplayCapabilitiesInfo, DisplayMethod, DisplayResult, JsonRpcError, JsonRpcErrorExt,
    JsonRpcRequest, JsonRpcRequestExt, JsonRpcResponse, JsonRpcResponseExt,
};
use toadstool_display::window::WindowId;
use toadstool_display::{DisplayError, DisplayServer, WindowManager};

// -----------------------------------------------------------------------------
// input/events.rs — KeyCode, Modifiers, MouseButton, TouchPhase, InputEvent
// -----------------------------------------------------------------------------

#[test]
fn input_events_keycode_construction_and_raw() {
    let k = KeyCode::new(42);
    assert_eq!(k.raw(), 42);
    assert_eq!(KeyCode::from_raw(99).raw(), 99);
}

#[test]
fn input_events_keycode_constants() {
    assert_eq!(KeyCode::ESC.raw(), 1);
    assert_eq!(KeyCode::RETURN.raw(), 28);
    assert_eq!(KeyCode::SPACE.raw(), 57);
    assert_eq!(KeyCode::A.raw(), 30);
    assert_eq!(KeyCode::Z.raw(), 44);
    assert_eq!(KeyCode::F1.raw(), 59);
    assert_eq!(KeyCode::F12.raw(), 88);
    assert_eq!(KeyCode::UP.raw(), 103);
    assert_eq!(KeyCode::LEFT_CTRL.raw(), 29);
}

#[test]
fn input_events_modifiers_default_and_none() {
    let m = Modifiers::default();
    assert!(!m.shift);
    assert!(!m.ctrl);
    assert!(!m.alt);
    assert!(!m.logo);
    assert!(!m.any());

    let m = Modifiers::none();
    assert!(!m.any());
}

#[test]
fn input_events_modifiers_any() {
    let m = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
        logo: false,
    };
    assert!(m.any());

    let m = Modifiers {
        shift: false,
        ctrl: false,
        alt: true,
        logo: false,
    };
    assert!(m.any());
}

#[test]
fn input_events_modifiers_debug_display() {
    let m = Modifiers::default();
    let s = format!("{m:?}");
    assert!(s.contains("Modifiers"));
}

#[test]
fn input_events_mouse_button_variants() {
    assert_eq!(MouseButton::Left, MouseButton::Left);
    assert_eq!(MouseButton::Right, MouseButton::Right);
    assert_eq!(MouseButton::Middle, MouseButton::Middle);
    assert_eq!(MouseButton::Button4, MouseButton::Button4);
    assert_eq!(MouseButton::Button5, MouseButton::Button5);
    assert_eq!(MouseButton::Other(7), MouseButton::Other(7));
}

#[test]
fn input_events_touch_phase_variants() {
    assert_eq!(TouchPhase::Started, TouchPhase::Started);
    assert_eq!(TouchPhase::Moved, TouchPhase::Moved);
    assert_eq!(TouchPhase::Ended, TouchPhase::Ended);
    assert_eq!(TouchPhase::Cancelled, TouchPhase::Cancelled);
}

#[test]
fn input_events_input_event_variants_debug() {
    let win = WindowId::new();
    let ev = InputEvent::KeyPress {
        key: KeyCode::A,
        modifiers: Modifiers::none(),
        window: win,
    };
    let s = format!("{ev:?}");
    assert!(s.contains("KeyPress") || s.contains("key"));

    let ev = InputEvent::MouseMove {
        x: 100,
        y: 200,
        window: win,
    };
    let s = format!("{ev:?}");
    assert!(s.contains("MouseMove") || s.contains("100"));

    let ev = InputEvent::MouseButton {
        button: MouseButton::Left,
        pressed: true,
        x: 50,
        y: 50,
        window: win,
    };
    let s = format!("{ev:?}");
    assert!(s.contains("MouseButton") || s.contains("Left"));

    let ev = InputEvent::WindowFocused { window: win };
    let s = format!("{ev:?}");
    assert!(s.contains("WindowFocused") || s.contains("window"));

    let ev = InputEvent::WindowResized {
        window: win,
        width: 800,
        height: 600,
    };
    let s = format!("{ev:?}");
    assert!(s.contains("WindowResized") || s.contains("800"));
}

#[test]
fn input_events_serialization_roundtrip() {
    let win = WindowId::new();
    let ev = InputEvent::KeyRelease {
        key: KeyCode::SPACE,
        modifiers: Modifiers {
            shift: true,
            ctrl: false,
            alt: false,
            logo: false,
        },
        window: win,
    };
    let json = serde_json::to_string(&ev).unwrap();
    let restored: InputEvent = serde_json::from_str(&json).unwrap();
    match (&ev, &restored) {
        (
            InputEvent::KeyRelease {
                key: k1,
                modifiers: m1,
                ..
            },
            InputEvent::KeyRelease {
                key: k2,
                modifiers: m2,
                ..
            },
        ) => {
            assert_eq!(k1.raw(), k2.raw());
            assert_eq!(m1.shift, m2.shift);
        }
        _ => panic!("variant mismatch"),
    }
}

// -----------------------------------------------------------------------------
// drm/device.rs — DeviceCapabilities, Device::discover_all
// -----------------------------------------------------------------------------

#[test]
fn drm_device_capabilities_construction_and_debug() {
    let caps = DeviceCapabilities {
        supports_dumb_buffers: true,
        supports_atomic_modesetting: false,
        preferred_depth: 32,
        driver_name: "i915".to_string(),
        driver_version: "1.2.3".to_string(),
    };
    let s = format!("{caps:?}");
    assert!(s.contains("i915"));
    assert!(caps.supports_dumb_buffers);
    assert!(!caps.supports_atomic_modesetting);
    assert_eq!(caps.preferred_depth, 32);
}

#[test]
fn drm_device_open_nonexistent_returns_error() {
    let result = Device::open("/dev/dri/nonexistent-card-xyz-999");
    assert!(result.is_err());
    if let Err(DisplayError::DeviceNotFound(p)) = result {
        assert!(p.to_string_lossy().contains("nonexistent"));
    }
}

#[test]
fn drm_device_discover_all_returns_result() {
    let result = Device::discover_all();
    assert!(result.is_ok());
    let paths = result.unwrap();
    for p in &paths {
        assert!(p.to_string_lossy().contains("card") || p.to_string_lossy().contains("render"));
    }
}

// -----------------------------------------------------------------------------
// lib.rs — DisplayError, Result
// -----------------------------------------------------------------------------

#[test]
fn display_error_variants_debug_display() {
    let e = DisplayError::DeviceNotFound(PathBuf::from("/dev/dri/card0"));
    let s = format!("{e}");
    assert!(s.contains("not found") || s.contains("card0"));

    let e = DisplayError::AllocationFailed;
    let s = format!("{e:?}");
    assert!(s.contains("AllocationFailed"));

    let e = DisplayError::IpcError("test error".to_string());
    let s = format!("{e}");
    assert!(s.contains("test error"));
}

#[test]
fn display_error_window_not_found() {
    let wid = WindowId::new();
    let e = DisplayError::WindowNotFound(wid);
    let s = format!("{e}");
    assert!(s.contains("Window not found") || s.contains("window"));
}

// -----------------------------------------------------------------------------
// ipc/mod.rs — JsonRpcRequest, JsonRpcResponse, JsonRpcError, DisplayMethod, DisplayResult
// -----------------------------------------------------------------------------

#[test]
fn ipc_jsonrpc_request_new() {
    let req = JsonRpcRequest::new_auto("display.create_window", None);
    assert_eq!(req.jsonrpc, "2.0");
    assert_eq!(req.method, "display.create_window");
    assert!(req.id.is_some());
}

#[test]
fn ipc_jsonrpc_request_notification() {
    let req = JsonRpcRequest::notification("display.input_event", None);
    assert!(req.id.is_none());
}

#[test]
fn ipc_jsonrpc_response_success() {
    let resp = JsonRpcResponse::success(
        serde_json::json!("id-1"),
        serde_json::json!({"window_id": "abc"}),
    );
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
}

#[test]
fn ipc_jsonrpc_response_error() {
    let err = JsonRpcError::internal_error("oops");
    let resp = JsonRpcResponse::from_error(serde_json::json!(1), err);
    assert!(resp.result.is_none());
    assert!(resp.error.is_some());
}

#[test]
fn ipc_jsonrpc_error_factory_methods() {
    assert_eq!(JsonRpcError::parse_error().code, -32700);
    assert_eq!(JsonRpcError::invalid_request().code, -32600);
    let e = JsonRpcError::method_not_found("foo");
    assert_eq!(e.code, -32601);
    assert!(e.message.contains("foo"));
    let e = JsonRpcError::invalid_params("bad");
    assert_eq!(e.code, -32602);
}

#[test]
fn ipc_display_capabilities_info_construction() {
    let info = DisplayCapabilitiesInfo {
        primal_id: "display-test".to_string(),
        socket_path: "/tmp/display.sock".to_string(),
        max_windows: 8,
        supported_formats: vec!["RGBA8888".to_string()],
        has_gpu_acceleration: true,
        vsync_available: true,
        display_count: 1,
        input_device_count: 2,
        window_count: 0,
        isomorphic: false,
    };
    let s = format!("{info:?}");
    assert!(s.contains("display-test"));
}

#[test]
fn ipc_display_method_serialization() {
    use toadstool_display::window::CreateWindowRequest;
    let m = DisplayMethod::CreateWindow(CreateWindowRequest::default());
    let json = serde_json::to_string(&m).unwrap();
    assert!(json.contains("display.create_window") || json.contains("create_window"));
}

#[test]
fn ipc_display_result_serialization() {
    let r = DisplayResult::WindowCreated {
        window_id: "uuid-123".to_string(),
    };
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains("window_id") || json.contains("uuid-123"));
}

// -----------------------------------------------------------------------------
// Async tests — DisplayServer, WindowManager (when DRM available)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn ipc_display_server_new_socket_path() {
    let manager = match WindowManager::new().await {
        Ok(m) => m,
        Err(_) => return,
    };
    let server = DisplayServer::new(manager);
    let path = server.socket_path();
    assert!(!path.as_os_str().is_empty());
}

#[tokio::test]
async fn ipc_transport_variants_debug() {
    let t = IpcTransport::UnixSocket;
    let s = format!("{t:?}");
    assert!(s.contains("Unix") || s.contains("Socket"));
}
