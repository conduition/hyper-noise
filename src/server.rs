use hyper::{Body, Request, Response};
use tokio::net::{TcpListener, TcpStream};
use tokio_noise::{handshakes, NoiseTcpStream};

use std::{
    borrow::Borrow,
    collections::HashMap,
    error::Error,
    future::Future,
    hash::Hash,
    ops::Deref as _,
    sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

use crate::ServerError;

pub trait PskList<Psk> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk>;
}

impl<Psk, B> PskList<Psk> for HashMap<B, Psk>
where
    B: Hash + Eq + Borrow<[u8]>,
    Psk: Clone,
{
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        self.get(id).cloned()
    }
}

impl<Psk, T: PskList<Psk>> PskList<Psk> for Mutex<T> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        self.lock().unwrap().find_psk(id)
    }
}
impl<Psk, T: PskList<Psk>> PskList<Psk> for RwLock<T> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        self.read().unwrap().find_psk(id)
    }
}
impl<Psk, T: PskList<Psk>> PskList<Psk> for MutexGuard<'_, T> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        self.deref().find_psk(id)
    }
}
impl<Psk, T: PskList<Psk>> PskList<Psk> for RwLockReadGuard<'_, T> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        self.deref().find_psk(id)
    }
}
impl<Psk, T: PskList<Psk>> PskList<Psk> for RwLockWriteGuard<'_, T> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        self.deref().find_psk(id)
    }
}
impl<Psk, T: PskList<Psk>> PskList<Psk> for Arc<T> {
    fn find_psk(&self, id: &[u8]) -> Option<Psk> {
        let v: &T = self.as_ref();
        v.find_psk(id)
    }
}

pub async fn serve_http<Psk, L, H, F, E>(
    tcp_stream: TcpStream,
    peers: &L,
    mut handle_request: H,
) -> Result<(), ServerError>
where
    L: PskList<Psk>,
    Psk: AsRef<[u8]>,
    H: FnMut(&[u8], Request<Body>) -> F,
    F: 'static + Send + Future<Output = Result<Response<Body>, E>>,
    E: Into<Box<dyn Error + Send + Sync>>,
{
    let mut responder = handshakes::nn_psk2::Responder::new(|id| peers.find_psk(id));
    let handshake = handshakes::NNpsk2::new(&mut responder);
    let noise_stream = NoiseTcpStream::handshake_responder(tcp_stream, handshake).await?;

    let peer_identity = responder
        .initiator_identity()
        .expect("initiator identity is always set after successful handshake")
        .to_owned();

    let http_service = hyper::service::service_fn(move |req| handle_request(&peer_identity, req));

    hyper::server::conn::Http::new()
        .serve_connection(noise_stream, http_service)
        .await?;
    Ok(())
}

pub async fn accept_and_serve_http<Psk, L, M, Svc, F, E>(
    listener: TcpListener,
    peers: L,
    mut make_handle_request: M,
) -> Result<(), std::io::Error>
where
    L: 'static + Send + Sync + Clone + PskList<Psk>,
    Psk: 'static + Send + Sync + AsRef<[u8]>,
    M: FnMut() -> Svc,
    Svc: 'static + Send + FnMut(&[u8], Request<Body>) -> F,
    F: 'static + Send + Future<Output = Result<Response<Body>, E>>,
    E: Into<Box<dyn Error + Send + Sync>>,
{
    loop {
        let peers = peers.clone();
        let handle_request: Svc = make_handle_request();

        let (tcp_stream, remote_addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => return Err(e)?,
        };

        tokio::task::spawn(async move {
            let peers = peers.clone();
            let result = serve_http(tcp_stream, &peers, handle_request).await;

            if let Err(e) = result {
                log::warn!(
                    "failed to serve HTTP request from {} over noise: {}",
                    remote_addr,
                    e
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Body, Request, Response};
    use std::{
        collections::HashMap,
        convert::Infallible,
        sync::{Arc, Mutex},
    };

    #[tokio::test]
    async fn noise_http_server_compiles_with_closure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let peers = HashMap::<Vec<u8>, [u8; 32]>::from([
            (Vec::from(b"alice"), [0u8; 32]),
            (Vec::from(b"bob"), [1u8; 32]),
            (Vec::from(b"charlie"), [2u8; 32]),
        ]);

        let make_handle_request = || {
            |peer_id: &[u8], _req: Request<Body>| async move {
                let _ = peer_id;
                let resp = Response::new(Body::empty());
                Ok::<_, Infallible>(resp)
            }
        };

        accept_and_serve_http(listener, Arc::new(peers), make_handle_request)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn noise_http_server_compiles_with_fn() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let peers = HashMap::<Vec<u8>, [u8; 32]>::from([
            (Vec::from(b"alice"), [0u8; 32]),
            (Vec::from(b"bob"), [1u8; 32]),
            (Vec::from(b"charlie"), [2u8; 32]),
        ]);

        async fn handle_request(
            _peer_name: &[u8],
            _req: Request<Body>,
        ) -> Result<Response<Body>, Infallible> {
            let resp = Response::new(Body::empty());
            Ok::<_, Infallible>(resp)
        }

        let make_handle_request = || {
            |peer_id: &[u8], req| {
                let peer_id = peer_id.to_vec();
                async move { handle_request(&peer_id, req).await }
            }
        };

        accept_and_serve_http(listener, Arc::new(peers), make_handle_request)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn noise_http_server_compiles_with_peers_in_mutex() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        let peers = Arc::new(Mutex::new(HashMap::<Vec<u8>, [u8; 32]>::from([
            (Vec::from(b"alice"), [0u8; 32]),
            (Vec::from(b"bob"), [1u8; 32]),
            (Vec::from(b"charlie"), [2u8; 32]),
        ])));

        let make_handle_request = || {
            |peer_id: &[u8], _req: Request<Body>| async move {
                let _ = peer_id;
                let resp = Response::new(Body::empty());
                Ok::<_, Infallible>(resp)
            }
        };

        accept_and_serve_http(listener, peers.clone(), make_handle_request)
            .await
            .unwrap();
    }
}
