use hyper::{Body, Request, Response};
use tokio::net::{TcpListener, TcpStream};
use tokio_noise::{
    handshakes::{nn_psk2::Responder, NNpsk2},
    NoiseTcpStream,
};

use std::{error::Error, future::Future};

use crate::ServerError;

pub async fn serve_http<Psk, P, H, F, E>(
    tcp_stream: TcpStream,
    mut responder: Responder<P, Psk>,
    mut handle_request: H,
) -> Result<(), ServerError>
where
    P: FnMut(&[u8]) -> Option<Psk>,
    Psk: AsRef<[u8]>,
    H: FnMut(&[u8], Request<Body>) -> F,
    F: 'static + Send + Future<Output = Result<Response<Body>, E>>,
    E: Into<Box<dyn Error + Send + Sync>>,
{
    let handshake = NNpsk2::new(&mut responder);
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

pub async fn accept_and_serve_http<Psk, P, M1, M2, Svc, F, E>(
    listener: TcpListener,
    mut make_responder: M1,
    mut make_handle_request: M2,
) -> Result<(), std::io::Error>
where
    M1: FnMut(std::net::SocketAddr) -> Responder<P, Psk>,
    P: 'static + Send + FnMut(&[u8]) -> Option<Psk>,
    Psk: 'static + Send + Sync + AsRef<[u8]>,
    M2: FnMut() -> Svc,
    Svc: 'static + Send + FnMut(&[u8], Request<Body>) -> F,
    F: 'static + Send + Future<Output = Result<Response<Body>, E>>,
    E: Into<Box<dyn Error + Send + Sync>>,
{
    loop {
        let handle_request: Svc = make_handle_request();

        let (tcp_stream, remote_addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => return Err(e)?,
        };

        let responder = make_responder(remote_addr);
        tokio::task::spawn(async move {
            let result = serve_http(tcp_stream, responder, handle_request).await;

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

    #[ignore]
    #[tokio::test]
    async fn compiles_with_closure() {
        let peers = Arc::new(HashMap::<Vec<u8>, [u8; 32]>::from([
            (Vec::from(b"alice"), [0u8; 32]),
            (Vec::from(b"bob"), [1u8; 32]),
            (Vec::from(b"charlie"), [2u8; 32]),
        ]));

        let make_responder = move |_| {
            let peers = peers.clone();
            Responder::new(move |id| peers.get(id).cloned())
        };

        let make_handle_request = || {
            |peer_id: &[u8], _req: Request<Body>| async move {
                let _ = peer_id;
                let resp = Response::new(Body::empty());
                Ok::<_, Infallible>(resp)
            }
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        accept_and_serve_http(listener, make_responder, make_handle_request)
            .await
            .unwrap();
    }

    #[ignore]
    #[tokio::test]
    async fn compiles_with_fn() {
        let peers = Arc::new(HashMap::<Vec<u8>, [u8; 32]>::from([
            (Vec::from(b"alice"), [0u8; 32]),
            (Vec::from(b"bob"), [1u8; 32]),
            (Vec::from(b"charlie"), [2u8; 32]),
        ]));

        let make_responder = move |_| {
            let peers = peers.clone();
            Responder::new(move |id| peers.get(id).cloned())
        };

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

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        accept_and_serve_http(listener, make_responder, make_handle_request)
            .await
            .unwrap();
    }

    #[ignore]
    #[tokio::test]
    async fn compiles_with_mutable_peers() {
        let peers = Arc::new(Mutex::new(HashMap::<Vec<u8>, [u8; 32]>::from([
            (Vec::from(b"alice"), [0u8; 32]),
            (Vec::from(b"bob"), [1u8; 32]),
            (Vec::from(b"charlie"), [2u8; 32]),
        ])));

        let make_responder = move |_| {
            let peers = peers.clone();
            Responder::new(move |id| peers.lock().unwrap().get(id).cloned())
        };

        let make_handle_request = || {
            |peer_id: &[u8], _req: Request<Body>| async move {
                let _ = peer_id;
                let resp = Response::new(Body::empty());
                Ok::<_, Infallible>(resp)
            }
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        accept_and_serve_http(listener, make_responder, make_handle_request)
            .await
            .unwrap();
    }
}
