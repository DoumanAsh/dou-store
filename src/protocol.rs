//! Storage uses JSON-RPC protocol

use std::collections::HashMap;

pub type RequestPayload = HashMap<String, serde_json::Value>;
///Request
pub type Request = json_rpc_types::Request<RequestPayload>;
///Response
pub type Response = json_rpc_types::Response<serde_json::Value, &'static str>;

///Character used to indicate end of message
pub const EOT: u8 = 0x04;
