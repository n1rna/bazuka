mod api;
mod context;
mod errors;
mod heartbeat;
mod http;
pub mod upnp;
use context::{NodeContext, TransactionStats};
pub use errors::NodeError;

#[cfg(feature = "pow")]
use context::Miner;

use crate::blockchain::Blockchain;
use crate::utils;
use crate::wallet::Wallet;
use hyper::{Body, Method, Request, Response, StatusCode};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use crate::config::punish;

use serde_derive::{Deserialize, Serialize};

use tokio::sync::RwLock;
use tokio::try_join;

pub type Timestamp = u32;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerAddress(pub IpAddr, pub u16); // ip, port

impl std::fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "http://{}:{}", self.0, self.1)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PeerInfo {
    pub height: usize,
    #[cfg(feature = "pow")]
    pub power: u64,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Peer {
    pub address: PeerAddress,
    pub punished_until: Timestamp,
    pub info: Option<PeerInfo>,
}

impl Peer {
    pub fn is_punished(&self) -> bool {
        utils::local_timestamp() < self.punished_until
    }
    pub fn punish(&mut self, secs: u32) {
        let now = utils::local_timestamp();
        self.punished_until = std::cmp::min(
            std::cmp::max(self.punished_until, now) + secs,
            now + punish::MAX_PUNISH,
        );
    }
}

async fn node_service<B: Blockchain>(
    _client: SocketAddr,
    context: Arc<RwLock<NodeContext<B>>>,
    req: Request<Body>,
) -> Result<Response<Body>, NodeError> {
    let mut response = Response::new(Body::empty());
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let qs = req.uri().query().unwrap_or("").to_string();
    let body = req.into_body();

    match (method, &path[..]) {
        // Miner will call this to fetch new PoW work.
        #[cfg(feature = "pow")]
        (Method::GET, "/miner/puzzle") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::get_miner_puzzle(Arc::clone(&context), serde_qs::from_str(&qs)?).await?,
            )?);
        }

        // Miner will call this when he has solved the PoW puzzle.
        #[cfg(feature = "pow")]
        (Method::POST, "/miner/solution") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::post_miner_solution(
                    Arc::clone(&context),
                    serde_json::from_slice(&hyper::body::to_bytes(body).await?)?,
                )
                .await?,
            )?);
        }

        // Register the miner software as a webhook.
        #[cfg(feature = "pow")]
        (Method::POST, "/miner") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::post_miner(
                    Arc::clone(&context),
                    serde_json::from_slice(&hyper::body::to_bytes(body).await?)?,
                )
                .await?,
            )?);
        }

        (Method::GET, "/stats") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::get_stats(Arc::clone(&context), serde_qs::from_str(&qs)?).await?,
            )?);
        }
        (Method::GET, "/peers") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::get_peers(Arc::clone(&context), serde_qs::from_str(&qs)?).await?,
            )?);
        }
        (Method::POST, "/peers") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::post_peer(
                    Arc::clone(&context),
                    serde_json::from_slice(&hyper::body::to_bytes(body).await?)?,
                )
                .await?,
            )?);
        }
        (Method::POST, "/bincode/transact") => {
            *response.body_mut() = Body::from(serde_json::to_vec(
                &api::transact(
                    Arc::clone(&context),
                    serde_json::from_slice(&hyper::body::to_bytes(body).await?)?,
                )
                .await?,
            )?);
        }
        (Method::GET, "/bincode/headers") => {
            *response.body_mut() = Body::from(bincode::serialize(
                &api::get_headers(Arc::clone(&context), serde_qs::from_str(&qs)?).await?,
            )?);
        }
        (Method::GET, "/bincode/blocks") => {
            *response.body_mut() = Body::from(bincode::serialize(
                &api::get_blocks(Arc::clone(&context), serde_qs::from_str(&qs)?).await?,
            )?);
        }
        (Method::POST, "/bincode/blocks") => {
            *response.body_mut() = Body::from(bincode::serialize(
                &api::post_block(
                    Arc::clone(&context),
                    bincode::deserialize(&hyper::body::to_bytes(body).await?)?,
                )
                .await?,
            )?);
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    };

    Ok(response)
}

use tokio::sync::mpsc;

pub struct IncomingRequest {
    pub socket_addr: SocketAddr,
    pub body: Request<Body>,
    pub resp: mpsc::Sender<Result<Response<Body>, NodeError>>,
}

pub struct OutgoingRequest {
    pub body: Request<Body>,
    pub resp: mpsc::Sender<Result<Response<Body>, NodeError>>,
}

pub struct OutgoingSender {
    chan: mpsc::UnboundedSender<OutgoingRequest>,
}

impl OutgoingSender {
    pub async fn raw(&self, body: Request<Body>) -> Result<Response<Body>, NodeError> {
        let (resp_snd, mut resp_rcv) = mpsc::channel::<Result<Response<Body>, NodeError>>(1);
        let req = OutgoingRequest {
            body,
            resp: resp_snd,
        };
        self.chan
            .send(req)
            .map_err(|_| NodeError::NotListeningError)?;
        resp_rcv.recv().await.ok_or(NodeError::NotAnsweringError)?
    }

    async fn bincode_get<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        addr: String,
        req: Req,
    ) -> Result<Resp, NodeError> {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("{}?{}", addr, serde_qs::to_string(&req)?))
            .body(Body::empty())?;
        let body = self.raw(req).await?.into_body();
        let resp: Resp = bincode::deserialize(&hyper::body::to_bytes(body).await?)?;
        Ok(resp)
    }

    #[allow(dead_code)]
    async fn bincode_post<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        addr: String,
        req: Req,
    ) -> Result<Resp, NodeError> {
        let req = Request::builder()
            .method(Method::POST)
            .uri(&addr)
            .header("content-type", "application/octet-stream")
            .body(Body::from(bincode::serialize(&req)?))?;
        let body = self.raw(req).await?.into_body();
        let resp: Resp = bincode::deserialize(&hyper::body::to_bytes(body).await?)?;
        Ok(resp)
    }

    async fn json_post<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        addr: String,
        req: Req,
    ) -> Result<Resp, NodeError> {
        let req = Request::builder()
            .method(Method::POST)
            .uri(&addr)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&req)?))?;
        let body = self.raw(req).await?.into_body();
        let resp: Resp = serde_json::from_slice(&hyper::body::to_bytes(body).await?)?;
        Ok(resp)
    }

    #[allow(dead_code)]
    async fn json_get<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        addr: String,
        req: Req,
    ) -> Result<Resp, NodeError> {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("{}?{}", addr, serde_qs::to_string(&req)?))
            .body(Body::empty())?;
        let body = self.raw(req).await?.into_body();
        let resp: Resp = serde_json::from_slice(&hyper::body::to_bytes(body).await?)?;
        Ok(resp)
    }
}

pub async fn node_request(
    chan: Arc<mpsc::UnboundedSender<IncomingRequest>>,
    client: SocketAddr,
    req: Request<Body>,
) -> Result<Response<Body>, NodeError> {
    let (resp_snd, mut resp_rcv) = mpsc::channel::<Result<Response<Body>, NodeError>>(1);
    let req = IncomingRequest {
        socket_addr: client,
        body: req,
        resp: resp_snd,
    };
    chan.send(req).map_err(|_| NodeError::NotListeningError)?;
    resp_rcv.recv().await.ok_or(NodeError::NotAnsweringError)?
}

pub async fn node_create<B: Blockchain>(
    address: PeerAddress,
    bootstrap: Vec<PeerAddress>,
    blockchain: B,
    wallet: Option<Wallet>,
    mut incoming: mpsc::UnboundedReceiver<IncomingRequest>,
    outgoing: mpsc::UnboundedSender<OutgoingRequest>,
) -> Result<(), NodeError> {
    let context = Arc::new(RwLock::new(NodeContext {
        outgoing: Arc::new(OutgoingSender { chan: outgoing }),
        blockchain,
        wallet,
        mempool: HashMap::new(),
        peers: bootstrap
            .into_iter()
            .map(|addr| {
                (
                    addr,
                    Peer {
                        address: addr,
                        punished_until: 0,
                        info: None,
                    },
                )
            })
            .collect(),
        timestamp_offset: 0,
        #[cfg(feature = "pow")]
        miner: None,
    }));

    let server_future = async {
        loop {
            if let Some(msg) = incoming.recv().await {
                if let Err(_) = msg
                    .resp
                    .send(node_service(msg.socket_addr, Arc::clone(&context), msg.body).await)
                    .await
                {}
            } else {
                break;
            }
        }
        Ok(())
    };

    let heartbeat_future = heartbeat::heartbeater(address, Arc::clone(&context));

    try_join!(server_future, heartbeat_future)?;
    Ok(())
}
