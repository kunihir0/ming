use bytes::{Buf, BytesMut};
use std::net::ToSocketAddrs;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

use crate::types::{A2sError, Result, ServerInfo};

pub struct A2sClient {
    timeout: Duration,
}

impl Default for A2sClient {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3),
        }
    }
}

impl A2sClient {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Queries a server for basic info (A2S_INFO)
    pub async fn info<A: ToSocketAddrs>(&self, addr: A) -> Result<ServerInfo> {
        let std_addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| A2sError::InvalidPacket("Could not resolve address".to_string()))?;

        // Bind to any local port
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        let mut req = Vec::new();
        req.extend_from_slice(b"\xFF\xFF\xFF\xFFTSource Engine Query\0");

        socket.send_to(&req, std_addr).await?;

        let mut buf = [0u8; 65536];
        let (len, _) = timeout(self.timeout, socket.recv_from(&mut buf)).await??;

        let mut data = BytesMut::from(&buf[..len]);

        // Check header (FF FF FF FF)
        if data.remaining() < 4 {
            return Err(A2sError::InvalidPacket("Packet too short".to_string()));
        }
        let header = data.get_i32_le();
        if header != -1 {
            return Err(A2sError::InvalidPacket("Invalid packet header".to_string()));
        }

        let mut packet_type = data.get_u8();

        // Handle Challenge for A2S_INFO
        if packet_type == 0x41 {
            let mut chal = vec![0; 4];
            data.copy_to_slice(&mut chal);

            req.extend_from_slice(&chal);
            socket.send_to(&req, std_addr).await?;

            let (len2, _) = timeout(self.timeout, socket.recv_from(&mut buf)).await??;
            data = BytesMut::from(&buf[..len2]);

            if data.remaining() < 5 {
                return Err(A2sError::InvalidPacket("Packet too short".to_string()));
            }
            let header2 = data.get_i32_le();
            if header2 != -1 {
                return Err(A2sError::InvalidPacket("Invalid packet header".to_string()));
            }
            packet_type = data.get_u8();
        }

        // 0x49 ('I') is the standard response for A2S_INFO
        if packet_type != 0x49 {
            return Err(A2sError::InvalidPacket(format!(
                "Unexpected packet type: {:#04x}",
                packet_type
            )));
        }

        let protocol = data.get_u8();
        let name = read_c_string(&mut data)?;
        let map = read_c_string(&mut data)?;
        let folder = read_c_string(&mut data)?;
        let game = read_c_string(&mut data)?;
        let app_id = data.get_u16_le();
        let players = data.get_u8();
        let max_players = data.get_u8();
        let bots = data.get_u8();
        let server_type = data.get_u8() as char;
        let environment = data.get_u8() as char;
        let visibility = data.get_u8();
        let vac = data.get_u8();
        let version = read_c_string(&mut data)?;

        let mut extra_data_flag = None;
        let mut keywords = None;
        let mut real_players = None;
        let mut real_max_players = None;

        if data.has_remaining() {
            let edf = data.get_u8();
            extra_data_flag = Some(edf);

            // 0x80: Port
            if (edf & 0x80) != 0 {
                if data.remaining() >= 2 {
                    data.advance(2);
                }
            }
            // 0x10: SteamID
            if (edf & 0x10) != 0 {
                if data.remaining() >= 8 {
                    data.advance(8);
                }
            }
            // 0x40: Spectator
            if (edf & 0x40) != 0 {
                if data.remaining() >= 2 {
                    data.advance(2);
                }
                let _ = read_c_string(&mut data);
            }
            // 0x20: Keywords
            if (edf & 0x20) != 0 {
                if let Ok(k) = read_c_string(&mut data) {
                    for tag in k.split(',') {
                        if let Some(cp_str) = tag.strip_prefix("cp") {
                            if let Ok(cp) = cp_str.parse::<u16>() {
                                real_players = Some(cp);
                            }
                        } else if let Some(mp_str) = tag.strip_prefix("mp") {
                            if let Ok(mp) = mp_str.parse::<u16>() {
                                real_max_players = Some(mp);
                            }
                        }
                    }
                    keywords = Some(k);
                }
            }
            // 0x01: GameID
            if (edf & 0x01) != 0 {
                if data.remaining() >= 8 {
                    data.advance(8);
                }
            }
        }

        Ok(ServerInfo {
            protocol,
            name,
            map,
            folder,
            game,
            app_id,
            players,
            max_players,
            bots,
            server_type,
            environment,
            visibility,
            vac,
            version,
            extra_data_flag,
            real_players,
            real_max_players,
            keywords,
        })
    }

    /// Queries a server for the active player list (A2S_PLAYER)
    pub async fn players<A: ToSocketAddrs>(&self, addr: A) -> Result<Vec<crate::types::Player>> {
        let std_addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| A2sError::InvalidPacket("Could not resolve address".to_string()))?;

        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        // 1. Initial request to get challenge
        let mut req = Vec::new();
        req.extend_from_slice(b"\xFF\xFF\xFF\xFFU\xFF\xFF\xFF\xFF");
        socket.send_to(&req, std_addr).await?;

        let mut buf = [0u8; 65536];
        let (len, _) = timeout(self.timeout, socket.recv_from(&mut buf)).await??;
        let mut data = BytesMut::from(&buf[..len]);

        if data.remaining() < 5 {
            return Err(A2sError::InvalidPacket(
                "Challenge packet too short".to_string(),
            ));
        }

        let header = data.get_i32_le();
        if header != -1 {
            return Err(A2sError::InvalidPacket("Invalid packet header".to_string()));
        }

        let packet_type = data.get_u8();
        let challenge = if packet_type == 0x41 {
            // 'A'
            // We got a challenge response, read 4 byte challenge
            let mut chal = vec![0; 4];
            data.copy_to_slice(&mut chal);
            chal
        } else {
            return Err(A2sError::InvalidPacket(
                "Expected challenge response".to_string(),
            ));
        };

        // 2. Send request with challenge
        let mut req_chal = Vec::new();
        req_chal.extend_from_slice(b"\xFF\xFF\xFF\xFFU");
        req_chal.extend_from_slice(&challenge);

        socket.send_to(&req_chal, std_addr).await?;

        let (len, _) = timeout(self.timeout, socket.recv_from(&mut buf)).await??;
        let mut data = BytesMut::from(&buf[..len]);

        if data.remaining() < 5 {
            return Err(A2sError::InvalidPacket(
                "Player payload packet too short".to_string(),
            ));
        }

        let header = data.get_i32_le();
        if header != -1 {
            return Err(A2sError::InvalidPacket("Invalid packet header".to_string()));
        }

        let packet_type = data.get_u8();
        if packet_type != 0x44 {
            // 'D'
            return Err(A2sError::InvalidPacket(format!(
                "Expected player payload response, got {:#04x}",
                packet_type
            )));
        }

        let player_count = data.get_u8();
        let mut players = Vec::new();

        for _ in 0..player_count {
            if data.remaining() < 1 {
                break;
            }
            let index = data.get_u8();
            let name = read_c_string(&mut data)?;
            if data.remaining() < 8 {
                break;
            }
            let score = data.get_i32_le();
            let duration = data.get_f32_le();

            players.push(crate::types::Player {
                index,
                name,
                score,
                duration,
            });
        }

        Ok(players)
    }
}

fn read_c_string(buf: &mut BytesMut) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        if !buf.has_remaining() {
            break;
        }
        let b = buf.get_u8();
        if b == 0 {
            break;
        }
        bytes.push(b);
    }

    String::from_utf8(bytes)
        .map_err(|_| A2sError::InvalidPacket("Invalid UTF-8 in string".to_string()))
}
