use prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tonic::codec::{BufferSettings, Codec, Decoder, Encoder};
use tonic::Status;

/// A gRPC codec for prost-reflect DynamicMessage.
///
/// Unlike tonic's ProstCodec which works with compile-time generated types,
/// this codec works with runtime-resolved message descriptors, enabling
/// dynamic RPC invocation without pre-compiled service stubs.
pub struct DynamicCodec {
    request_desc: MessageDescriptor,
    response_desc: MessageDescriptor,
}

impl DynamicCodec {
    pub fn new(request_desc: MessageDescriptor, response_desc: MessageDescriptor) -> Self {
        DynamicCodec {
            request_desc,
            response_desc,
        }
    }
}

impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder {
            _request_desc: self.request_desc.clone(),
        }
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder {
            response_desc: self.response_desc.clone(),
        }
    }
}

/// Encodes DynamicMessage into protobuf wire format.
pub struct DynamicEncoder {
    _request_desc: MessageDescriptor,
}

impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut tonic::codec::EncodeBuf<'_>,
    ) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|e| Status::internal(format!("failed to encode request: {e}")))?;
        Ok(())
    }

    fn buffer_settings(&self) -> BufferSettings {
        BufferSettings::default()
    }
}

/// Decodes protobuf wire format into DynamicMessage.
pub struct DynamicDecoder {
    response_desc: MessageDescriptor,
}

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(
        &mut self,
        src: &mut tonic::codec::DecodeBuf<'_>,
    ) -> Result<Option<Self::Item>, Self::Error> {
        let msg = DynamicMessage::decode(self.response_desc.clone(), src)
            .map_err(|e| Status::internal(format!("failed to decode response: {e}")))?;
        Ok(Some(msg))
    }

    fn buffer_settings(&self) -> BufferSettings {
        BufferSettings::default()
    }
}
