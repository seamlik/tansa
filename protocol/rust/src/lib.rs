//! gRPC protocol.

tonic::include_proto!("tw.seamlik.tansa.v1");

use crate::multicast_packet::Payload;
use prost::bytes::BytesMut;
use prost::Message;
use std::marker::PhantomData;
use thiserror::Error;
use tokio_util::codec::Decoder;

#[derive(Default)]
pub struct ProtobufDecoder<M> {
    phantom: PhantomData<M>,
}

impl<M> Decoder for ProtobufDecoder<M>
where
    M: Message + Default,
{
    type Item = M;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        Some(M::decode(src)).transpose().map_err(Into::into)
    }
}

impl MulticastPacket {
    pub fn unwrap_request(self) -> Option<Request> {
        if let Some(Payload::Request(r)) = self.payload {
            Some(r)
        } else {
            None
        }
    }
    pub fn unwrap_response(self) -> Option<Response> {
        if let Some(Payload::Announcement(r)) = self.payload {
            Some(r)
        } else {
            None
        }
    }
}

impl From<Request> for MulticastPacket {
    fn from(value: Request) -> Self {
        Self {
            payload: Some(Payload::Request(value)),
        }
    }
}

impl From<Response> for MulticastPacket {
    fn from(value: Response) -> Self {
        Self {
            payload: Some(Payload::Announcement(value)),
        }
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
        let actual_request = ProtobufDecoder::default().decode(&mut bytes).unwrap();
        assert_eq!(actual_request, Some(request));
    }

    #[test]
    fn decode_empty_request() {
        let actual_request: Option<Request> = ProtobufDecoder::default()
            .decode(&mut Default::default())
            .unwrap();
        assert_eq!(actual_request, None);
    }
}
