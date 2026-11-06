//! Checking that a download is exactly the release GitHub described.

use crate::Error;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

/// The largest update download accepted.
pub const MAX_DOWNLOAD: u64 = 100 * 1024 * 1024;

/// The size and SHA-256 digest a downloaded file must match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedFile {
    pub size: u64,
    pub sha256: String,
}

impl ExpectedFile {
    /// Accepts a non-empty size within the limit and a 64-digit hex digest.
    pub fn new(size: u64, sha256: &str) -> Option<Self> {
        let valid = size > 0
            && size <= MAX_DOWNLOAD
            && sha256.len() == 64
            && sha256.bytes().all(|byte| byte.is_ascii_hexdigit());
        valid.then(|| Self {
            size,
            sha256: sha256.to_ascii_lowercase(),
        })
    }

    /// Copies `input` to `output`, failing unless the bytes match exactly.
    pub fn copy(&self, mut input: impl Read, mut output: impl Write) -> Result<(), Error> {
        let mut digest = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = input
                .read(&mut buffer)
                .map_err(|e| format!("Could not read the update: {e}"))?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > self.size || total > MAX_DOWNLOAD {
                return Err("The update download is larger than GitHub reported.".into());
            }
            digest.update(&buffer[..count]);
            output
                .write_all(&buffer[..count])
                .map_err(|e| format!("Could not save the update: {e}"))?;
        }
        if total != self.size || format!("{:x}", digest.finalize()) != self.sha256 {
            return Err(
                "The update download failed its SHA-256 verification. Please try again.".into(),
            );
        }
        Ok(())
    }

    /// Re-checks a file already on disk.
    pub fn verify(&self, path: &Path) -> Result<(), Error> {
        self.copy(File::open(path)?, std::io::sink())
    }
}

/// Fails unless `path` starts with the headers of a Windows x64 executable.
pub fn check_windows_x64(path: &Path) -> Result<(), Error> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|_| "The update is not a Windows executable.")?;
    if &header[..2] != b"MZ" {
        return Err("The update is not a Windows executable.".into());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
    if offset > 1024 * 1024 {
        return Err("The update has an invalid executable header.".into());
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)
        .map_err(|_| "The update has an invalid executable header.")?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664 {
        return Err("The update is not a Windows x64 executable.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread, time::Duration};

    fn expected(contents: &[u8], size: u64) -> ExpectedFile {
        ExpectedFile::new(size, &format!("{:x}", Sha256::digest(contents))).unwrap()
    }

    #[test]
    fn downloads_must_match_the_reported_size_and_checksum() {
        let contents = b"a verified executable";
        let size = contents.len() as u64;
        let mut copied = Vec::new();
        expected(contents, size)
            .copy(&contents[..], &mut copied)
            .unwrap();
        assert_eq!(copied, contents);
        assert!(expected(contents, size)
            .copy(&contents[..contents.len() - 1], std::io::sink())
            .is_err());
        assert!(expected(contents, size - 1)
            .copy(&contents[..], std::io::sink())
            .is_err());
        assert!(ExpectedFile::new(size, &"0".repeat(64))
            .unwrap()
            .copy(&contents[..], std::io::sink())
            .is_err());

        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("lost connection"))
            }
        }
        assert!(expected(contents, 1)
            .copy(Broken, std::io::sink())
            .unwrap_err()
            .message()
            .contains("lost connection"));
    }

    #[test]
    fn digests_must_be_sha256_hex() {
        assert!(
            ExpectedFile::new(1, &"A".repeat(64)).is_some_and(|file| file.sha256 == "a".repeat(64))
        );
        assert!(ExpectedFile::new(1, "wrong").is_none());
        assert!(ExpectedFile::new(0, &"a".repeat(64)).is_none());
        assert!(ExpectedFile::new(MAX_DOWNLOAD + 1, &"a".repeat(64)).is_none());
    }

    #[test]
    fn interrupted_http_body_fails_after_copying_only_partial_staged_data() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let partial = b"partial update payload";
        let declared_size = partial.len() as u64 + 4096;
        let server = thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0u8; 2048];
            let _ = connection.read(&mut request);
            write!(
                connection,
                "HTTP/1.1 200 OK\r\nContent-Length: {declared_size}\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            connection.write_all(partial).unwrap();
            // Closing before the declared body is complete makes reqwest report a
            // real transport failure while the body is being copied.
        });

        let folder = tempfile::tempdir().unwrap();
        let staged = folder.path().join("partial-update.exe");
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let response = client
            .get(format!("http://{address}/update.exe"))
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
        let mut output = File::create(&staged).unwrap();
        let result = expected(partial, declared_size).copy(response, &mut output);
        output.flush().unwrap();
        drop(output);
        server.join().unwrap();

        let error = result.unwrap_err();
        assert!(
            error.message().contains("Could not read the update"),
            "expected a response-body transport error, got: {error}"
        );
        assert_eq!(std::fs::read(&staged).unwrap(), partial);
        assert!(
            std::fs::metadata(&staged).unwrap().len() < declared_size,
            "only bytes received before the disconnect may be staged"
        );
    }

    #[test]
    fn update_must_be_a_windows_x64_executable() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("new.exe");
        std::fs::write(&path, b"<html>download failed</html>").unwrap();
        assert!(check_windows_x64(&path).is_err());
        let mut executable = vec![0u8; 128];
        executable[..2].copy_from_slice(b"MZ");
        executable[60..64].copy_from_slice(&64u32.to_le_bytes());
        executable[64..70].copy_from_slice(b"PE\0\0\x64\x86");
        std::fs::write(&path, &executable).unwrap();
        check_windows_x64(&path).unwrap();
        executable[68..70].copy_from_slice(&0x14cu16.to_le_bytes());
        std::fs::write(&path, &executable).unwrap();
        assert!(check_windows_x64(&path).is_err());
    }
}
