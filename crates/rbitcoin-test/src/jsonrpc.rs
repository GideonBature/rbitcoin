//! Bitcoin Core JSON-RPC 1.0 over one HTTP/1.1 POST.
//!
//! Connect failure is `Err` so a poll can retry. A body that is not JSON panics.

use serde_json::Value;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const RPC_BEARER: &str = "Bearer pass";

/// POST `method`. `Err` only when the TCP connect fails.
pub async fn try_jsonrpc(addr: SocketAddr, method: &str, params: Value) -> std::io::Result<Value> {
    let body = serde_json::json!({"jsonrpc":"1.0","id":"test","method":method,"params":params})
        .to_string();
    let req = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\nAuthorization: {RPC_BEARER}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let mut stream = TcpStream::connect(addr).await?;
    stream
        .write_all(req.as_bytes())
        .await
        .unwrap_or_else(|e| panic!("rpc write: {e}"));
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .unwrap_or_else(|e| panic!("rpc read: {e}"));
    let text = String::from_utf8_lossy(&buf);
    let json_body = text.split("\r\n\r\n").nth(1).unwrap_or("").trim();
    Ok(serde_json::from_str(json_body)
        .unwrap_or_else(|e| panic!("rpc {method} json: {e} body={json_body}")))
}

/// [`try_jsonrpc`] that panics when the connect fails.
pub async fn jsonrpc(addr: SocketAddr, method: &str, params: Value) -> Value {
    try_jsonrpc(addr, method, params)
        .await
        .unwrap_or_else(|e| panic!("rpc {method} connect: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn try_jsonrpc_returns_the_connect_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let err = try_jsonrpc(addr, "getblockcount", serde_json::json!([]))
            .await
            .expect_err("refused connect");
        assert_eq!(err.kind(), std::io::ErrorKind::ConnectionRefused);
    }

    #[tokio::test]
    #[should_panic(expected = "rpc getblockcount json:")]
    async fn try_jsonrpc_panics_when_the_body_is_not_json() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 2048];
            let _ = sock.read(&mut buf).await;
            let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope";
            sock.write_all(resp).await.unwrap();
        });
        let _ = try_jsonrpc(addr, "getblockcount", serde_json::json!([])).await;
    }
}
