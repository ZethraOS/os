// pdu.rs — GSM 7-bit and SMS PDU encoding for AetherOS Telephony
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code, unused_parens)]

use anyhow::Result;

pub struct PduEncoder;

impl PduEncoder {
    pub fn encode_7bit(text: &str) -> Vec<u8> {
        let mut octets = Vec::new();
        let septets = text.chars().map(|c| c as u8 & 0x7F).collect::<Vec<u8>>();

        let mut current_byte = 0u8;
        let mut shift = 0;

        for septet in septets {
            current_byte |= septet << shift;
            shift += 7;

            if shift >= 8 {
                octets.push(current_byte);
                shift -= 8;
                current_byte = septet >> (7 - shift);
            }
        }

        if shift > 0 {
            octets.push(current_byte);
        }

        octets
    }

    pub fn decode_7bit(octets: &[u8]) -> String {
        let mut text = String::new();
        let mut shift = 0;
        let mut current_byte = 0u8;

        for &byte in octets {
            current_byte |= byte << shift;
            text.push((current_byte & 0x7F) as char);
            current_byte = byte >> (7 - shift);
            shift += 1;

            if shift == 7 {
                text.push((current_byte & 0x7F) as char);
                shift = 0;
                current_byte = 0;
            }
        }

        text
    }

    pub fn create_submit_pdu(number: &str, text: &str) -> Result<String> {
        let encoded_text = Self::encode_7bit(text);
        let mut pdu = format!("000100{:02X}91", number.len());
        for chunk in number.as_bytes().chunks(2) {
            if chunk.len() == 2 {
                pdu.push(chunk[1] as char);
                pdu.push(chunk[0] as char);
            } else {
                pdu.push('F');
                pdu.push(chunk[0] as char);
            }
        }
        pdu.push_str("0000FF");
        pdu.push_str(&format!("{:02X}", text.len()));
        pdu.push_str(&hex::encode(encoded_text));

        Ok(pdu.to_uppercase())
    }
}
