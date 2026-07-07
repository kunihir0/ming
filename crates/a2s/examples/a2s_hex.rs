use bytes::Buf;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
    let addr = "us.monthly.rplayrust.com:28017";

    let mut req = Vec::new();
    req.extend_from_slice(b"\xFF\xFF\xFF\xFFTSource Engine Query\0");

    socket.send_to(&req, addr).await.unwrap();

    let mut buf = [0u8; 1400];
    let (len, _) = timeout(Duration::from_secs(3), socket.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();

    let mut data = bytes::BytesMut::from(&buf[..len]);

    if data[4] == 0x41 {
        let mut chal = vec![0; 4];
        data.advance(5);
        data.copy_to_slice(&mut chal);
        req.extend_from_slice(&chal);
        socket.send_to(&req, addr).await.unwrap();
        let (len2, _) = timeout(Duration::from_secs(3), socket.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        data = bytes::BytesMut::from(&buf[..len2]);
    }

    println!("Raw Packet: {:02X?}", &data[..]);
}
