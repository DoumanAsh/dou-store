use std::net;

use rogu::error;
use json_rpc_types::{Id, Error, Version, ErrorCode};
use xxhash_rust::xxh3::xxh3_64;
use xxhash_rust::const_xxh3::xxh3_64 as const_xxh3_64;

use crate::db;
use crate::protocol::{Request, RequestPayload, Response};

//methods
const PING: u64 = const_xxh3_64(b"ping");
const CHECKSUM: u64 = const_xxh3_64(b"cheksum");
const CONFIG: u64 = const_xxh3_64(b"config");
const SET_CONFIG: u64 = const_xxh3_64(b"set_config");

//params
const ID: &'static str = "id";
const DATA: &'static str = "data";
const RESULT: &'static str = "result";

const LOCAL_HOST: net::IpAddr = net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1));

mod int_err {
    pub const CHECKSUM_FAIL_GET: i64 = 1;
    pub const CONFIG_FAIL_GET: i64 = 10;
    pub const CONFIG_RSP_CORRUPT: i64 = 20;
    pub const SET_CONFIG_FAIL: i64 = 30;
    pub const SET_CONFIG_SERDE_FAIL: i64 = 31;
    pub const TASK_SPAWN_FAIL: i64 = 100;
}

pub mod tcp;

#[derive(Clone)]
struct Handler {
    db: db::DbView,
}

#[inline]
fn invalid_req(msg: &'static str, id: Option<Id>) -> Response {
    Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data(msg), id)
}

#[inline]
const fn internal_err(err: i64, id: Option<Id>) -> Response {
    Response::error(Version::V2, Error::from_code(ErrorCode::ServerError(err)), id)
}

#[inline]
fn checksum_response(num: u64, id: Option<Id>) -> Response {
    let mut payload = serde_json::map::Map::with_capacity(1);
    payload.insert(RESULT.to_owned(), num.into());
    Response::result(Version::V2, payload.into(), id)
}

fn config_response(data: &[u8], id: Option<Id>) -> Response {
    let data = match core::str::from_utf8(data) {
        Ok(data) => data,
        Err(error) => {
            error!("Data corruption in config. Unexpected non-utf8 config: {}", error);
            return internal_err(int_err::CONFIG_RSP_CORRUPT, id)
        }
    };

    let mut payload = serde_json::map::Map::with_capacity(1);
    payload.insert(RESULT.to_owned(), data.into());
    Response::result(Version::V2, payload.into(), id)
}


#[inline]
fn handle_set_config_req(db: db::DbView, params: RequestPayload, id: Option<Id>) -> Response {
    let key = match params.get(ID) {
        Some(serde_json::Value::String(value)) => value,
        Some(_) => return invalid_req("Params field 'id' must be a string", id),
        None => return invalid_req("Params is missing field 'id'", id),
    };

    match params.get(DATA) {
        Some(serde_json::Value::String(value)) => set_config_response(db, key, value, id),
        //We prefer user to serialize, but accept object too.
        Some(serde_json::Value::Object(value)) => match serde_json::to_string(value) {
            Ok(value) => set_config_response(db, key, &value, id),
            Err(error) => {
                error!("Internal error serializing json: {}", error);
                internal_err(int_err::SET_CONFIG_SERDE_FAIL, id)
            },
        },
        Some(_) => invalid_req("Params field 'data' must be a string or object", id),
        None => invalid_req("Params is missing field 'data'", id),
    }
}

fn set_config_response(db: db::DbView, key: &str, value: &str, id: Option<Id>) -> Response {
    use sled::Transactional;
    use sled::transaction::TransactionError;

    let hash = xxh3_64(value.as_bytes());

    let result: Result<(), TransactionError<bool>> = (&db.checksum, &db.config).transaction(|(checksum, config)| {
        checksum.insert(key.as_bytes(), &hash.to_be_bytes())?;
        config.insert(key.as_bytes(), value.as_bytes())?;
        Ok(())
    });

    match result {
        Ok(_) => checksum_response(hash, id),
        Err(error) => {
            error!("Unable to set config: {}", error);
            return internal_err(int_err::SET_CONFIG_FAIL, id);
        }
    }
}


#[inline]
fn handle_checksum_req(db: db::DbView, params: RequestPayload, id: Option<Id>) -> Response {
    match params.get(ID) {
        Some(serde_json::Value::String(value)) => match db.checksum.get(&value) {
            Ok(Some(value)) => {
                let mut bytes = [0u8; 8];
                bytes.clone_from_slice(&value);
                checksum_response(u64::from_be_bytes(bytes), id)
            },
            Ok(None) => checksum_response(0, id),
            Err(error) => {
                error!("Internal error accessing checksum tree: {}", error);
                internal_err(int_err::CHECKSUM_FAIL_GET, id)
            }
        },
        Some(_) => invalid_req("Params field 'id' must be a string", id),
        None => invalid_req("Params is missing field 'id'", id),
    }
}


#[inline]
fn handle_config_req(db: db::DbView, params: RequestPayload, id: Option<Id>) -> Response {
    match params.get(ID) {
        Some(serde_json::Value::String(value)) => match db.config.get(&value) {
            Ok(Some(value)) => config_response(&value, id),
            Ok(None) => config_response(&[], id),
            Err(error) => {
                error!("Internal error accessing config tree: {}", error);
                internal_err(int_err::CONFIG_FAIL_GET, id)
            },
        },
        Some(_) => invalid_req("Params field 'id' must be a string", id),
        None => invalid_req("Params is missing field 'id'", id),
    }
}

impl Handler {
    pub const fn new(db: db::DbView) -> Self {
        Self {
            db,
        }
    }

    async fn handle_request(&self, request: Request) -> Response {
        match xxh3_64(request.method.as_str().as_bytes()) {
            PING => Response::result(Version::V2, Default::default(), request.id),
            CHECKSUM => match request.params {
                Some(params) => {
                    let id = request.id.clone();
                    let db = self.db.clone();
                    match tokio::task::spawn_blocking(move || handle_checksum_req(db, params, id)).await {
                        Ok(result) => result,
                        Err(error) => {
                            error!("Failed to execute handle_checksum_req task: {}", error);
                            internal_err(int_err::TASK_SPAWN_FAIL, request.id)
                        }
                    }
                },
                None => invalid_req("Missing params", request.id),
            },
            CONFIG => match request.params {
                Some(params) => {
                    let id = request.id.clone();
                    let db = self.db.clone();
                    match tokio::task::spawn_blocking(move || handle_config_req(db, params, id)).await {
                        Ok(result) => result,
                        Err(error) => {
                            error!("Failed to execute handle_config_req task: {}", error);
                            internal_err(int_err::TASK_SPAWN_FAIL, request.id)
                        }
                    }
                },
                None => invalid_req("Missing params", request.id),
            },
            SET_CONFIG => match request.params {
                Some(params) => {
                    let id = request.id.clone();
                    let db = self.db.clone();
                    match tokio::task::spawn_blocking(move || handle_set_config_req(db, params, id)).await {
                        Ok(result) => result,
                        Err(error) => {
                            error!("Failed to execute handle_set_config_req task: {}", error);
                            internal_err(int_err::TASK_SPAWN_FAIL, request.id)
                        }
                    }
                },
                None => invalid_req("Missing params", request.id),
            },
            _ => Response::error(Version::V2, Error::from_code(ErrorCode::MethodNotFound), request.id),
        }
    }
}
