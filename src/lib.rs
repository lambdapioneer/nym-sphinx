// Copyright 2020 Nym Technologies SA
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use curve25519_dalek::scalar::Scalar;

use crate::constants::{DESTINATION_ADDRESS_LENGTH, PAYLOAD_SIZE, SECURITY_PARAMETER};
use crate::header::delays::Delay;
use crate::header::{ProcessedHeader, SphinxHeader, SphinxUnwrapError, HEADER_SIZE};
use crate::payload::Payload;
use crate::route::{Destination, DestinationAddressBytes, Node, NodeAddressBytes, SURBIdentifier};

pub mod constants;
pub mod crypto;
pub mod header;
pub mod key;
pub mod payload;
pub mod route;
mod utils;

pub const PACKET_SIZE: usize = HEADER_SIZE + PAYLOAD_SIZE;

#[derive(Debug, PartialEq)]
pub enum ProcessingError {
    InvalidRoutingInformationLengthError,
    InvalidHeaderLengthError,
    InvalidPayloadLengthError,
    InvalidPacketLengthError,
}

pub enum ProcessedPacket {
    // TODO: considering fields sizes here (`SphinxPacket` and `Payload`), we perhaps
    // should follow clippy recommendation and box it
    ProcessedPacketForwardHop(SphinxPacket, NodeAddressBytes, Delay),
    ProcessedPacketFinalHop(DestinationAddressBytes, SURBIdentifier, Payload),
}

#[derive(Clone)]
pub struct SphinxPacket {
    pub header: header::SphinxHeader,
    pub payload: Payload,
}

impl SphinxPacket {
    pub fn new(
        message: Vec<u8>,
        route: &[Node],
        destination: &Destination,
        delays: &[Delay],
    ) -> Result<SphinxPacket, SphinxUnwrapError> {
        let initial_secret = crypto::generate_secret();
        let (header, payload_keys) =
            header::SphinxHeader::new(initial_secret, route, delays, destination);

        if message.len() + DESTINATION_ADDRESS_LENGTH > PAYLOAD_SIZE - SECURITY_PARAMETER {
            return Err(SphinxUnwrapError::NotEnoughPayload);
        }
        let payload =
            Payload::encapsulate_message(&message, &payload_keys, destination.address.clone())?;
        Ok(SphinxPacket { header, payload })
    }

    // TODO: we should have some list of 'seen shared_keys' for replay detection, but this should be handled by a mix node
    pub fn process(self, node_secret_key: Scalar) -> Result<ProcessedPacket, SphinxUnwrapError> {
        let unwrapped_header = self.header.process(node_secret_key)?;
        match unwrapped_header {
            ProcessedHeader::ProcessedHeaderForwardHop(
                new_header,
                next_hop_address,
                delay,
                payload_key,
            ) => {
                let new_payload = self.payload.unwrap(&payload_key);
                let new_packet = SphinxPacket {
                    header: new_header,
                    payload: new_payload,
                };
                Ok(ProcessedPacket::ProcessedPacketForwardHop(
                    new_packet,
                    next_hop_address,
                    delay,
                ))
            }
            ProcessedHeader::ProcessedHeaderFinalHop(destination, identifier, payload_key) => {
                let new_payload = self.payload.unwrap(&payload_key);
                Ok(ProcessedPacket::ProcessedPacketFinalHop(
                    destination,
                    identifier,
                    new_payload,
                ))
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.header
            .to_bytes()
            .iter()
            .cloned()
            .chain(self.payload.get_content_ref().iter().cloned())
            .collect()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProcessingError> {
        if bytes.len() != PACKET_SIZE {
            return Err(ProcessingError::InvalidPacketLengthError);
        }

        let header_bytes = &bytes[..HEADER_SIZE];
        let payload_bytes = &bytes[HEADER_SIZE..];
        let header = SphinxHeader::from_bytes(header_bytes)?;
        let payload = Payload::from_bytes(payload_bytes)?;

        Ok(SphinxPacket { header, payload })
    }
}

#[cfg(test)]
mod test_building_packet_from_bytes {
    use super::*;

    #[test]
    fn from_bytes_returns_error_if_bytes_are_too_short() {
        let bytes = [0u8; 1];
        let expected = ProcessingError::InvalidPacketLengthError;
        match SphinxPacket::from_bytes(&bytes) {
            Err(err) => assert_eq!(expected, err),
            _ => panic!("Should have returned an error when packet bytes too short"),
        };
    }

    #[test]
    fn from_bytes_panics_if_bytes_are_too_long() {
        let bytes = [0u8; 6666];
        let expected = ProcessingError::InvalidPacketLengthError;
        match SphinxPacket::from_bytes(&bytes) {
            Err(err) => assert_eq!(expected, err),
            _ => panic!("Should have returned an error when packet bytes too long"),
        };
    }
}
