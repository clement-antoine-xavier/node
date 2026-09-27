//! Connecting to peers.

use tonic::transport::{Channel, Endpoint};

use crate::error::NetError;

/// Create a gRPC channel to `uri`, e.g. `http://127.0.0.1:9002`.
pub async fn connect_channel(uri: &str) -> Result<Channel, NetError> {
    let endpoint = Endpoint::from_shared(uri.to_owned()).map_err(|error| NetError::Address {
        address: uri.to_owned(),
        reason: error.to_string(),
    })?;
    Ok(endpoint.connect().await?)
}
