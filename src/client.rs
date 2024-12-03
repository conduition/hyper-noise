use hyper::{Body, Request, Response};
use tokio::net::TcpStream;
use tokio_noise::{
    handshakes::{nn_psk2::Initiator, NNpsk2},
    NoiseTcpStream,
};

use std::time::Duration;

use crate::ClientError;

pub async fn send_request(
    tcp_stream: TcpStream,
    initiator: Initiator<'_>,
    request: Request<Body>,
    request_timeout: Option<Duration>,
) -> Result<Response<Body>, ClientError> {
    let handshake = NNpsk2::new(initiator);

    let timeout = request_timeout.unwrap_or(Duration::from_secs(999999999));
    tokio::time::timeout(timeout, async move {
        let noise_stream = NoiseTcpStream::handshake_initiator(tcp_stream, handshake).await?;

        let (mut request_sender, connection) = hyper::client::conn::handshake(noise_stream).await?;
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                log::warn!("Error in hyper HTTP connection driver: {}", e);
            }
        });

        let resp = request_sender.send_request(request).await?;
        Ok(resp)
    })
    .await
    .map_err(|_| ClientError::RequestTimeout)?
}
