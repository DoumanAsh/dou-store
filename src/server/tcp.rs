use tokio::net::{TcpStream, TcpListener};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use rogu::{info, warn, trace};

use super::{Handler, LOCAL_HOST};
use crate::protocol::{Request, EOT};
use crate::db;

pub async fn handle_client(db: db::DbView, socket: TcpStream, _addr: std::net::SocketAddr) {
    let handler = Handler::new(db);

    let mut serde_buf = Vec::<u8>::new();
    let mut read_buf = Vec::new();
    let mut socket = BufReader::new(socket);

    loop {
        match socket.read_until(EOT, &mut read_buf).await {
            Ok(0) => {
                trace!("{}: TCP disconnect", _addr);
                break;
            },
            Ok(_) => (),
            Err(_error) => {
                trace!("{}: TCP error: {}", _addr, _error);
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
                        trace!("{}: Unable to send response: {}", _addr, _error);
                    }
                }

                serde_buf.clear()
            },
            Err(_error) => {
                trace!("{}: Invalid request: {}", _addr, _error);
            },
        }

        read_buf.clear();
    }
}

pub struct Server {
    port: u16,
    db: db::DbView,
}

impl Server {
    pub const fn new(port: u16, db: db::DbView) -> Self {
        Self {
            port,
            db,
        }
    }

    pub async fn start(&self) -> bool {
        let mut serv = match TcpListener::bind((LOCAL_HOST, self.port)).await {
            Ok(serv) => serv,
            Err(error) => {
                warn!("Unable to start TCP server on {}:{}. Error: {}", LOCAL_HOST, self.port, error);
                return false;
            }
        };

        info!("Start TCP on {}:{}", LOCAL_HOST, self.port);

        loop {
            let (socket, _addr) = match serv.accept().await {
                Ok(res) => res,
                Err(error) => {
                    warn!("TCP Error: {}", error);
                    return false
                }
            };


            trace!("{}: Connected over TCP", _addr);

            tokio::spawn(handle_client(self.db.clone(), socket, _addr));
        }
    }
}
