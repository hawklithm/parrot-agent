//! AWS Signature Version 4 signing primitives (shared by S3 storage and
//! Secrets Manager providers). Self-contained HMAC-SHA256 chain — no AWS SDK.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

pub fn hex_hmac(key: &[u8], data: &[u8]) -> String {
    let bytes = hmac_sha256(key, data);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// SigV4 signer for a fixed access key / secret key / region / service.
pub struct SigV4Signer {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub service: String,
}

impl SigV4Signer {
    pub fn new(access_key: String, secret_key: String, region: String, service: String) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            service,
        }
    }

    /// Derive the signing key for a date stamp (YYYYMMDD).
    pub fn signing_key(&self, date_stamp: &str) -> Vec<u8> {
        let date_key = hmac_sha256(format!("AWS4{}", self.secret_key).as_bytes(), date_stamp.as_bytes());
        let region_key = hmac_sha256(&date_key, self.region.as_bytes());
        let service_key = hmac_sha256(&region_key, self.service.as_bytes());
        hmac_sha256(&service_key, b"aws4_request")
    }

    /// Compute the final signature from a canonical request.
    pub fn signature(&self, canonical_request: &str, amz_date: &str, date_stamp: &str) -> String {
        let scope = format!("{date_stamp}/{}/{}", self.region, self.service);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}/aws4_request\n{}",
            hex_sha256(canonical_request.as_bytes())
        );
        let signing_key = self.signing_key(date_stamp);
        hex_hmac(&signing_key, string_to_sign.as_bytes())
    }

    /// Authorization header value for a signed request.
    pub fn authorization_header(
        &self,
        canonical_request: &str,
        amz_date: &str,
        date_stamp: &str,
        signed_headers: &str,
    ) -> String {
        let signature = self.signature(canonical_request, amz_date, date_stamp);
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{date_stamp}/{}/{}/aws4_request, SignedHeaders={signed_headers}, Signature={signature}",
            self.access_key, self.region, self.service,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigv4_signature_matches_aws_test_vector() {
        // AWS SigV4 documentation example: GET /test.txt on examplebucket,
        // 2013-05-24T00:00:00Z, us-east-1, service s3.
        let signer = SigV4Signer::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "us-east-1".to_string(),
            "s3".to_string(),
        );
        let empty_sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let canonical = format!(
            "GET\n/test.txt\n\nhost:examplebucket.s3.amazonaws.com\nrange:bytes=0-9\nx-amz-content-sha256:{empty_sha}\nx-amz-date:20130524T000000Z\n\nhost;range;x-amz-content-sha256;x-amz-date\n{empty_sha}"
        );
        let signature = signer.signature(&canonical, "20130524T000000Z", "20130524");
        assert_eq!(signature.len(), 64);
        assert_eq!(
            signature,
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
            "documented AWS example signature must be reproduced"
        );
    }

    #[test]
    fn urlencode_escapes_non_unreserved() {
        assert_eq!(urlencode("AKIA/key"), "AKIA%2Fkey");
        assert_eq!(urlencode("a~b_c-d.e"), "a~b_c-d.e");
    }
}
