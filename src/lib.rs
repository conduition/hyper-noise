pub mod client;
pub mod server;

mod errors;
pub use errors::*;

#[cfg(test)]
mod tests {
    use super::*;

    use hyper::{Body, Request, Response};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_noise::handshakes::nn_psk2::{Initiator, Responder};

    use std::convert::Infallible;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn basic_get_request() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        const PSK: [u8; 32] = [0xFFu8; 32];

        let handle = tokio::task::spawn(async move {
            let make_responder = |_| {
                Responder::new(|id| match std::str::from_utf8(id) {
                    Ok("bob") => Some(PSK),
                    _ => None,
                })
            };
            let make_handle_request = |_| {
                |peer_id: &[u8], _req: Request<Body>| async move {
                    let _ = peer_id;
                    let resp = Response::new(Body::empty());
                    Ok::<_, Infallible>(resp)
                }
            };

            server::accept_and_serve_http(
                listener,
                make_responder,
                make_handle_request,
                Some(Duration::from_millis(250)),
            )
            .await
            .unwrap();
        });

        let initiator = Initiator {
            identity: "bob".as_ref(),
            psk: &PSK,
        };

        let request = Request::builder()
            .uri(format!("http://{listener_addr}"))
            .method("GET")
            .body(Body::from(vec![]))
            .unwrap();

        let tcp_stream = TcpStream::connect(listener_addr).await.unwrap();
        let response = client::send_request(tcp_stream, initiator, request, None)
            .await
            .unwrap();

        assert_eq!(response.status(), hyper::StatusCode::OK);

        handle.abort();
        handle.await.ok();
    }

    #[tokio::test]
    async fn server_handler_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        const PSK: [u8; 32] = [0xFFu8; 32];

        let handle = tokio::task::spawn(async move {
            let make_responder = |_| {
                Responder::new(|id| match std::str::from_utf8(id) {
                    Ok("bob") => Some(PSK),
                    _ => None,
                })
            };

            #[allow(unreachable_code)]
            let make_handle_request = |_| {
                |_peer_id: &[u8], _req: Request<Body>| async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    panic!("should never wait this long");
                    let resp = Response::new(Body::empty());
                    Ok::<_, Infallible>(resp)
                }
            };

            server::accept_and_serve_http(
                listener,
                make_responder,
                make_handle_request,
                Some(Duration::from_millis(250)),
            )
            .await
            .unwrap();
        });

        let initiator = Initiator {
            identity: "bob".as_ref(),
            psk: &PSK,
        };

        let request = Request::builder()
            .uri(format!("http://{listener_addr}"))
            .method("GET")
            .body(Body::from(vec![]))
            .unwrap();

        let start = Instant::now();

        let tcp_stream = TcpStream::connect(listener_addr).await.unwrap();

        // client has no timeout
        match client::send_request(tcp_stream, initiator, request, None).await {
            Err(ClientError::Hyper(hyper_err)) => assert!(hyper_err.is_incomplete_message()),
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("client request returned OK unexpectedly"),
        };

        handle.abort();
        handle.await.ok();

        assert!(start.elapsed().as_millis() < 350);
    }

    #[tokio::test]
    async fn client_request_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener_addr = listener.local_addr().unwrap();

        const PSK: [u8; 32] = [0xFFu8; 32];

        let handle = tokio::task::spawn(async move {
            let make_responder = |_| {
                Responder::new(|id| match std::str::from_utf8(id) {
                    Ok("bob") => Some(PSK),
                    _ => None,
                })
            };

            let make_handle_request = |_| {
                |_peer_id: &[u8], _req: Request<Body>| async move {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let resp = Response::new(Body::empty());
                    Ok::<_, Infallible>(resp)
                }
            };

            // Server has no timeout
            server::accept_and_serve_http(listener, make_responder, make_handle_request, None)
                .await
                .unwrap();
        });

        let initiator = Initiator {
            identity: "bob".as_ref(),
            psk: &PSK,
        };

        let request = Request::builder()
            .uri(format!("http://{listener_addr}"))
            .method("GET")
            .body(Body::from(vec![]))
            .unwrap();

        let start = Instant::now();

        let tcp_stream = TcpStream::connect(listener_addr).await.unwrap();
        match client::send_request(
            tcp_stream,
            initiator,
            request,
            Some(Duration::from_millis(250)),
        )
        .await
        {
            Err(ClientError::RequestTimeout) => { /* expected */ }
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("client request returned OK unexpectedly"),
        };

        assert!(start.elapsed().as_millis() < 350);

        handle.abort();
        handle.await.ok();
    }
}
