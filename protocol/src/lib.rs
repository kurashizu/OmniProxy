use anyhow::Result;
use bytes::{BufMut, Bytes, BytesMut};

pub const TYPE_TCP_CONNECT: u8 = 0x01;
pub const TYPE_TCP_CONNECTED: u8 = 0x02;
pub const TYPE_TCP_DATA: u8 = 0x03;
pub const TYPE_TCP_FIN: u8 = 0x04;
pub const TYPE_UDP_DATA: u8 = 0x05;
pub const TYPE_ICMP_DATA: u8 = 0x06;

pub fn encode_frame(stream_id: u32, typ: u8, payload: &[u8]) -> Bytes {
    let mut buf = BytesMut::with_capacity(5 + payload.len());
    buf.put_u32(stream_id);
    buf.put_u8(typ);
    buf.put_slice(payload);
    buf.freeze()
}

pub fn decode_frame(data: &[u8]) -> Result<(u32, u8, Bytes)> {
    if data.len() < 5 {
        anyhow::bail!("frame too short: {}B", data.len());
    }
    let stream_id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let typ = data[4];
    let payload = Bytes::copy_from_slice(&data[5..]);
    Ok((stream_id, typ, payload))
}

pub fn encode_udp_payload(host: &str, port: u16, data: &[u8]) -> Bytes {
    let hb = host.as_bytes();
    let mut buf = BytesMut::with_capacity(4 + hb.len() + data.len());
    buf.put_u16(hb.len() as u16);
    buf.put_slice(hb);
    buf.put_u16(port);
    buf.put_slice(data);
    buf.freeze()
}

pub fn decode_udp_payload(payload: &[u8]) -> Result<(String, u16, Bytes)> {
    if payload.len() < 4 {
        anyhow::bail!("udp payload too short");
    }
    let hl = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if payload.len() < 2 + hl + 2 {
        anyhow::bail!("udp payload truncated");
    }
    let host = String::from_utf8_lossy(&payload[2..2 + hl]).to_string();
    let port = u16::from_be_bytes([payload[2 + hl], payload[3 + hl]]);
    let data = Bytes::copy_from_slice(&payload[4 + hl..]);
    Ok((host, port, data))
}

pub fn encode_icmp_payload(ip: &str, data: &[u8]) -> Bytes {
    let ib = ip.as_bytes();
    let mut buf = BytesMut::with_capacity(2 + ib.len() + data.len());
    buf.put_u16(ib.len() as u16);
    buf.put_slice(ib);
    buf.put_slice(data);
    buf.freeze()
}

pub fn decode_icmp_payload(payload: &[u8]) -> Result<(String, Bytes)> {
    if payload.len() < 2 {
        anyhow::bail!("icmp payload too short");
    }
    let il = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if payload.len() < 2 + il {
        anyhow::bail!("icmp payload truncated");
    }
    let ip = String::from_utf8_lossy(&payload[2..2 + il]).to_string();
    let data = Bytes::copy_from_slice(&payload[2 + il..]);
    Ok((ip, data))
}
