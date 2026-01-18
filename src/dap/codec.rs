//! DAP wire protocol codec
//!
//! The DAP protocol uses HTTP-style headers followed by JSON body:
//! ```text
//! Content-Length: <byte-length>\r\n
//! \r\n
//! <JSON body>
//! ```

use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::common::Error;

/// Read a DAP message from the stream
///
/// Parses the Content-Length header and reads the JSON body
pub async fn read_message<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<String, Error> {
    // Read headers line by line until we get an empty line
    let mut content_length: Option<usize> = None;

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await.map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                Error::AdapterCrashed
            } else {
                Error::Io(e)
            }
        })?;

        if bytes_read == 0 {
            return Err(Error::AdapterCrashed);
        }

        // Empty line (just \r\n) signals end of headers
        if line == "\r\n" || line == "\n" {
            break;
        }

        // Parse Content-Length header
        let line = line.trim();
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse().map_err(|_| {
                Error::DapProtocol(format!("Invalid Content-Length: {}", value.trim()))
            })?);
        }
        // Ignore other headers (like Content-Type)
    }

    let len = content_length.ok_or_else(|| {
        Error::DapProtocol("Missing Content-Length header".to_string())
    })?;

    // Sanity check - 100MB should be plenty for any DAP message
    if len > 100 * 1024 * 1024 {
        return Err(Error::DapProtocol(format!(
            "Content-Length too large: {} bytes",
            len
        )));
    }

    // Read the JSON body
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await.map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            Error::AdapterCrashed
        } else {
            Error::Io(e)
        }
    })?;

    String::from_utf8(body).map_err(|e| Error::DapProtocol(format!("Invalid UTF-8: {}", e)))
}

/// Write a DAP message to the stream
///
/// Adds the Content-Length header and writes the JSON body
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    json: &str,
) -> Result<(), Error> {
    let header = format!("Content-Length: {}\r\n\r\n", json.len());

    writer.write_all(header.as_bytes()).await?;
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_read_message() {
        let data = b"Content-Length: 13\r\n\r\n{\"test\":true}";
        let mut reader = BufReader::new(Cursor::new(data.to_vec()));

        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, "{\"test\":true}");
    }

    #[tokio::test]
    async fn test_read_message_with_extra_headers() {
        let data = b"Content-Length: 13\r\nContent-Type: application/json\r\n\r\n{\"test\":true}";
        let mut reader = BufReader::new(Cursor::new(data.to_vec()));

        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, "{\"test\":true}");
    }

    #[tokio::test]
    async fn test_write_message() {
        let mut output = Vec::new();
        write_message(&mut output, "{\"test\":true}").await.unwrap();

        let expected = "Content-Length: 13\r\n\r\n{\"test\":true}";
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }
}
