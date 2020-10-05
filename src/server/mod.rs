use std::net;

use json_rpc_types::{Id, Error, Version, ErrorCode};
use rogu::error;

use crate::db;
use crate::protocol::{Request, RequestPayload, Response};

///methods
const CHECKSUM: &'static str = "cheksum";
const CONFIG: &'static str = "config";
const SET_CONFIG: &'static str = "set_config";

//params
const ID: &'static str = "id";
const DATA: &'static str = "data";

const LOCAL_HOST: net::IpAddr = net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1));

pub mod tcp;

struct Handler {
    db: db::DbView,
}

impl Handler {
    pub const fn new(db: db::DbView) -> Self {
        Self {
            db,
        }
    }

    #[inline]
    fn checksum_response(&self, num: u64, id: Option<Id>) -> Response {
        let mut payload = serde_json::map::Map::with_capacity(1);
        payload.insert("result".to_owned(), num.into());
        Response::result(Version::V2, payload.into(), id)
    }

    fn config_response(&self, data: &[u8], id: Option<Id>) -> Response {
        let data = match core::str::from_utf8(data) {
            Ok(data) => data,
            Err(error) => {
                error!("Data corruption in config. Unexpected non-utf8 config: {}", error);
                return Response::error(Version::V2, Error::from_code(ErrorCode::ServerError(2)), id)
            }
        };

        let mut payload = serde_json::map::Map::with_capacity(1);
        payload.insert("result".to_owned(), data.into());
        Response::result(Version::V2, payload.into(), id)
    }

    fn set_config_response(&self, key: &str, value: &str, id: Option<Id>) -> Response {
        use sled::Transactional;
        use sled::transaction::TransactionError;

        let hash = xxhash_c::xxh3_64(value.as_bytes());

        let result: Result<(), TransactionError<bool>> = (&self.db.checksum, &self.db.config).transaction(|(checksum, config)| {
            checksum.insert(key.as_bytes(), &hash.to_be_bytes())?;
            config.insert(key.as_bytes(), value.as_bytes())?;
            Ok(())
        });

        match result {
            Ok(_) => self.checksum_response(hash, id),
            Err(error) => {
                error!("Unable to set config: {}", error);
                return Response::error(Version::V2, Error::from_code(ErrorCode::ServerError(4)), id)
            }
        }
    }

    #[inline]
    fn handle_checksum_req(&self, params: RequestPayload, id: Option<Id>) -> Response {
        match params.get(ID) {
            Some(serde_json::Value::String(value)) => match self.db.checksum.get(&value) {
                Ok(Some(value)) => {
                    let mut bytes = [0u8; 8];
                    bytes.clone_from_slice(&value);
                    self.checksum_response(u64::from_be_bytes(bytes), id)
                },
                Ok(None) => self.checksum_response(0, id),
                Err(error) => {
                    error!("Internal error accessing checksum tree: {}", error);
                    Response::error(Version::V2, Error::from_code(ErrorCode::ServerError(1)), id)
                }
            },
            Some(_) => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params field 'id' must be a string"), id),
            None => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params is missing field 'id'"), id),
        }
    }

    #[inline]
    fn handle_set_config_req(&self, params: RequestPayload, id: Option<Id>) -> Response {
        let key = match params.get(ID) {
            Some(serde_json::Value::String(value)) => value,
            Some(_) => return Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params field 'id' must be a string"), id),
            None => return Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params is missing field 'id'"), id),
        };

        match params.get(DATA) {
            Some(serde_json::Value::String(value)) => self.set_config_response(key, value, id),
            //We prefer user to serialize, but accept object too.
            Some(serde_json::Value::Object(value)) => match serde_json::to_string(value) {
                Ok(value) => self.set_config_response(key, &value, id),
                Err(error) => {
                    error!("Internal error serializing json: {}", error);
                    Response::error(Version::V2, Error::from_code(ErrorCode::ServerError(3)), id)
                },
            },
            Some(_) => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params field 'data' must be a string or object"), id),
            None => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params is missing field 'data'"), id),
        }
    }

    #[inline]
    fn handle_config_req(&self, params: RequestPayload, id: Option<Id>) -> Response {
        match params.get(ID) {
            Some(serde_json::Value::String(value)) => match self.db.config.get(&value) {
                Ok(Some(value)) => self.config_response(&value, id),
                Ok(None) => self.config_response(&[], id),
                Err(error) => {
                    error!("Internal error accessing config tree: {}", error);
                    Response::error(Version::V2, Error::from_code(ErrorCode::ServerError(1)), id)
                },
            },
            Some(_) => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params field 'id' must be a string"), id),
            None => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Params is missing field 'id'"), id),
        }
    }

    async fn handle_request(&self, request: Request) -> Response {
        match request.method.as_str() {
            "ping" => Response::result(Version::V2, Default::default(), request.id),
            CHECKSUM => match request.params {
                Some(params) => self.handle_checksum_req(params, request.id),
                None => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Missing params"), request.id)
            },
            CONFIG => match request.params {
                Some(params) => self.handle_config_req(params, request.id),
                None => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Missing params"), request.id)
            },
            SET_CONFIG => match request.params {
                Some(params) => self.handle_set_config_req(params, request.id),
                None => Response::error(Version::V2, Error::from_code(ErrorCode::InvalidRequest).set_data("Missing params"), request.id)
            },
            _ => Response::error(Version::V2, Error::from_code(ErrorCode::MethodNotFound), request.id),
        }
    }
}
