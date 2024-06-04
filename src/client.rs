use hyper::{Body, Request, Response};
use tokio::net::TcpStream;
use tokio_noise::{handshakes, NoiseTcpStream};

use crate::ClientError;

pub async fn send_request(
    tcp_stream: TcpStream,
    our_id: impl AsRef<[u8]>,
    psk: impl AsRef<[u8]>,
    request: Request<Body>,
) -> Result<Response<Body>, ClientError> {
    let initiator = handshakes::nn_psk2::Initiator {
        psk: psk.as_ref(),
        identity: our_id.as_ref(),
    };
    let handshake = handshakes::NNpsk2::new(initiator);
    let noise_stream = NoiseTcpStream::handshake_initiator(tcp_stream, handshake).await?;

    let (mut request_sender, connection) = hyper::client::conn::handshake(noise_stream).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            log::warn!("Error in hyper HTTP connection driver: {}", e);
        }
    });

    let resp = request_sender.send_request(request).await?;
    Ok(resp)
}
