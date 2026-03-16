// signing.rs — Cryptographic verification for AetherOS OTA
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use base64::{engine::general_purpose, Engine as _};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::info;

pub struct SignatureVerifier {
    verifying_key: VerifyingKey,
}

impl SignatureVerifier {
    pub fn new(public_key_hex: &str) -> Result<Self> {
        let bytes = hex::decode(public_key_hex).context("Failed to decode public key hex")?;
        let key_bytes: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid public key length"))?;
        let verifying_key = VerifyingKey::from_bytes(&key_bytes).context("Invalid public key bytes")?;
        Ok(Self { verifying_key })
    }

    pub fn verify(&self, data: &[u8], signature_b64: &str) -> bool {
        let sig_bytes = match general_purpose::STANDARD.decode(signature_b64) {
            Ok(b) => b,
            Err(_) => return false,
        };
        
        let signature = match Signature::from_slice(&sig_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };

        match self.verifying_key.verify(data, &signature) {
            Ok(_) => {
                info!("Signature verification PASSED");
                true
            }
            Err(_) => {
                info!("Signature verification FAILED");
                false
            }
        }
    }

    pub async fn verify_payload_sha256(path: &Path, expected_sha256: &str) -> Result<()> {
        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 65536];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual_hash = format!("{:x}", hasher.finalize());
        if actual_hash != expected_sha256 {
            anyhow::bail!("Payload hash mismatch: expected {} got {}", expected_sha256, actual_hash);
        }
        info!("Payload SHA-256 verification PASSED");
        Ok(())
    }
}
