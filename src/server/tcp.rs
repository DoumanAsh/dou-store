use std::io;
use std::sync::Arc;
use std::collections::HashSet;
use core::future::Future;

use tokio::net::{TcpStream, TcpListener};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use rogu::{info, warn, trace};

use super::{Handler, LOCAL_HOST};
use crate::protocol::{Request, EOT};
use crate::db;

trait ErrorKindExt {
    ///Returns true whether error can be ignored in context of `TcpListener::accept`
    fn is_accept_error_ok(self) -> bool;
}

impl ErrorKindExt for io::ErrorKind {
    #[inline(always)]
    fn is_accept_error_ok(self) -> bool {
        self == io::ErrorKind::ConnectionAborted ||
        self == io::ErrorKind::ConnectionRefused ||
        self == io::ErrorKind::ConnectionReset ||
        self == io::ErrorKind::NotConnected ||
        self == io::ErrorKind::WouldBlock ||
        self == io::ErrorKind::TimedOut ||
        self == io::ErrorKind::Interrupted
    }
}

pub struct Tcp {
    server: Arc<Server>,
}

impl Tcp {
    #[inline]
    pub fn new(port: u16, db: db::DbView) -> Self {
        Self {
            server: Arc::new(Server::new(port, db)),
        }
    }

    #[inline]
    pub fn start(&self) -> impl Future<Output=bool> {
        self.server.clone().start()
    }
}

pub struct Server {
    port: u16,
    db: db::DbView,
    connected: tokio::sync::RwLock<HashSet<std::net::IpAddr>>,
}

impl Server {
    pub fn new(port: u16, db: db::DbView) -> Self {
        Self {
            port,
            db,
            connected: tokio::sync::RwLock::new(HashSet::new()),
        }
    }

    pub async fn handle_client(self: Arc<Self>, socket: TcpStream, addr: std::net::SocketAddr) {
        let handler = Handler::new(self.db.clone());

        let mut serde_buf = Vec::<u8>::new();
        let mut read_buf = Vec::new();
        let mut socket = BufReader::new(socket);

        loop {
            match socket.read_until(EOT, &mut read_buf).await {
                Ok(0) => {
                    trace!("{}: TCP disconnect", addr);
                    break;
                },
                Ok(_) => (),
                Err(_error) => {
                    trace!("{}: TCP error: {}", addr, _error);
                    break;
                }
            };

            match serde_json::from_slice::<Request>(&read_buf) {
                Ok(request) => {
                    if request.is_notification() {
                        //Nothing to notify about right now.
                        continue;
                    }

                    let response = handler.handle_request(request).await;
                    match serde_json::to_writer(&mut serde_buf, &response) {
                        Ok(_) => (),
                        Err(_) => unreachable!(),
                    };

                    match BufReader::get_mut(&mut socket).write_all(&serde_buf).await {
                        Ok(_) => (),
                        Err(_error) => {
                            trace!("{}: Unable to send response: {}", addr, _error);
                        }
                    }

                    serde_buf.clear()
                },
                Err(_error) => {
                    trace!("{}: Invalid request: {}", addr, _error);
                },
            }

            read_buf.clear();
        }

        self.connected.write().await.remove(&addr.ip());
    }

    pub async fn start(self: Arc<Self>) -> bool {
        let serv = match TcpListener::bind((LOCAL_HOST, self.port)).await {
            Ok(serv) => serv,
            Err(error) => {
                warn!("Unable to start TCP server on {}:{}. Error: {}", LOCAL_HOST, self.port, error);
                return false;
            }
        };

        info!("Start TCP on {}:{}", LOCAL_HOST, self.port);

        loop {
            let (socket, addr) = match serv.accept().await {
                Ok(res) => res,
                Err(error) => {
                    if error.kind().is_accept_error_ok() {
                        continue;
                    } else {
                        //TODO: do we need to clean connected here?
                        //Likely client tasks would just error
                        warn!("TCP Server Error: {}", error);
                        return false
                    }
                }
            };

            if self.connected.write().await.insert(addr.ip()) {
                trace!("{}: Connected over TCP", addr);

                tokio::spawn(self.clone().handle_client(socket, addr));
            } else {
                drop(socket);
                trace!("{}: Already connected over TCP", addr);
            }
        }
    }
}
