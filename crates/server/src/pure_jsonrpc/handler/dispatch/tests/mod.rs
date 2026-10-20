// SPDX-License-Identifier: AGPL-3.0-or-later

mod core_dispatch;
mod envelope;
mod fan_out;
mod orchestrator;
mod shader;
mod submit;
mod trio_contract;

use std::sync::Arc;

use super::DispatchHandler;
use crate::pure_jsonrpc::types::JsonRpcError;

fn test_handler() -> DispatchHandler {
    DispatchHandler::new(
        Arc::new(crate::visualization_client::VisualizationClient::unavailable()),
        None,
    )
}

fn submit_params(bdf: &str, dispatch_mode: &str) -> serde_json::Value {
    serde_json::json!({
        "binary": [1u8, 2, 3],
        "bdf": bdf,
        "dispatch_mode": dispatch_mode,
    })
}
