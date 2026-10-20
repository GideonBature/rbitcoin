use rbitcoin_test::jsonrpc::try_jsonrpc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
