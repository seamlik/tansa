//! gRPC protocol.

tonic::include_proto!("tw.seamlik.tansa.v1");

use prost::bytes::BytesMut;
use prost::Message;
use thiserror::Error;
use tokio_util::codec::Decoder;

pub struct RequestDecoder;

impl Decoder for RequestDecoder {
    type Item = Request;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        Some(Request::decode(src)).transpose().map_err(Into::into)
    }
}

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Error from network I/O")]
    Io(#[from] std::io::Error),

    #[error("Failed to decode as Protocol Buffers")]
    Protobuf(#[from] prost::DecodeError),
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn decode_request() {
        let request = Request {
            response_collector_port: 1,
        };
        let mut bytes = request.encode_to_vec().as_slice().into();
        let actual_request = RequestDecoder.decode(&mut bytes).unwrap();
        assert_eq!(actual_request, Some(request));
    }

    #[test]
    fn decode_empty_request() {
        let actual_request = RequestDecoder.decode(&mut Default::default()).unwrap();
        assert_eq!(actual_request, None);
    }
}
