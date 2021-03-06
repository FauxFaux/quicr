use bytes::{Buf, BufMut, BigEndian};

use {VERSION, Side};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TransportParameters {
    pub initial_max_stream_data: u32,
    pub initial_max_data: u32,
    pub idle_timeout: u16,
    /// Mandatory for servers
    pub stateless_reset_token: Option<[u8; 16]>,
    pub initial_max_streams_bidi: u16,
    pub initial_max_streams_uni: u16,
    pub max_packet_size: Option<u16>,
    pub ack_delay_exponent: u8,
}

const DEFAULT_ACK_DELAY_EXPONENT: u8 = 3;

impl Default for TransportParameters {
    fn default() -> Self { Self {
        // TODO: Sanity check all
        initial_max_stream_data: 64 * 1024,
        initial_max_data: 64 * 1024,
        idle_timeout: 10,
        stateless_reset_token: None,
        initial_max_streams_bidi: 0,
        initial_max_streams_uni: 0,
        max_packet_size: None,
        ack_delay_exponent: DEFAULT_ACK_DELAY_EXPONENT,
    }}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Fail)]
pub enum Error {
    #[fail(display = "version negotiation was tampered with")]
    VersionNegotiation,
    #[fail(display = "parameter had illegal value")]
    IllegalValue,
    #[fail(display = "parameters were malformed")]
    Malformed,
}

impl TransportParameters {
    pub fn write<W: BufMut>(&self, w: &mut W) {
        if self.stateless_reset_token.is_some() { // We're the server
            w.put_u32::<BigEndian>(VERSION); // Negotiated version
            w.put_u8(8);                     // Bytes of supported versions
            w.put_u32::<BigEndian>(0x0a1a2a3a); // Reserved version
            w.put_u32::<BigEndian>(VERSION); // Real supported version
        } else {
            w.put_u32::<BigEndian>(VERSION); // Initially requested version
        }
        
        let mut buf = Vec::with_capacity(22);

        buf.put_u16::<BigEndian>(0x0000);
        buf.put_u16::<BigEndian>(4);
        buf.put_u32::<BigEndian>(self.initial_max_stream_data);

        buf.put_u16::<BigEndian>(0x0001);
        buf.put_u16::<BigEndian>(4);
        buf.put_u32::<BigEndian>(self.initial_max_data);

        buf.put_u16::<BigEndian>(0x0003);
        buf.put_u16::<BigEndian>(2);
        buf.put_u16::<BigEndian>(self.idle_timeout);

        if let Some(ref x) = self.stateless_reset_token {
            buf.put_u16::<BigEndian>(0x0006);
            buf.put_u16::<BigEndian>(16);
            buf.put_slice(x);
        }

        if self.initial_max_streams_bidi != 0 {
            buf.put_u16::<BigEndian>(0x0002);
            buf.put_u16::<BigEndian>(2);
            buf.put_u16::<BigEndian>(self.initial_max_streams_bidi);
        }

        if self.initial_max_streams_uni != 0 {
            buf.put_u16::<BigEndian>(0x0008);
            buf.put_u16::<BigEndian>(2);
            buf.put_u16::<BigEndian>(self.initial_max_streams_uni);
        }

        if let Some(x) = self.max_packet_size {
            buf.put_u16::<BigEndian>(0x0005);
            buf.put_u16::<BigEndian>(2);
            buf.put_u16::<BigEndian>(x);
        }

        if self.ack_delay_exponent != DEFAULT_ACK_DELAY_EXPONENT {
            buf.put_u16::<BigEndian>(0x0007);
            buf.put_u16::<BigEndian>(1);
            buf.put_u8(self.ack_delay_exponent);
        }

        w.put_u16::<BigEndian>(buf.len() as u16);
        w.put_slice(&buf);
    }

    pub fn read<R: Buf>(side: Side, r: &mut R) -> Result<Self, Error> {
        if side == Side::Server {
            if r.remaining() < 26 { return Err(Error::Malformed); }
            // We only support one version, so there is no validation to do here.
            r.get_u32::<BigEndian>();
        } else {
            if r.remaining() < 31 { return Err(Error::Malformed); }
            let negotiated = r.get_u32::<BigEndian>();
            if negotiated != VERSION { return Err(Error::VersionNegotiation); }
            let supported_bytes = r.get_u8();
            if supported_bytes < 4 || supported_bytes > 252 || supported_bytes % 4 != 0 {
                return Err(Error::Malformed);
            }
            let mut found = false;
            for _ in 0..(supported_bytes / 4) {
                found |= r.get_u32::<BigEndian>() == negotiated;
            }
            if !found { return Err(Error::VersionNegotiation); }
        }

        let mut initial_max_stream_data = false;
        let mut initial_max_data = false;
        let mut idle_timeout = false;
        let mut initial_max_streams_bidi = false;
        let mut initial_max_streams_uni = false;
        let mut ack_delay_exponent = false;
        let mut params = Self::default();
        let params_len = r.get_u16::<BigEndian>();
        if params_len as usize != r.remaining() { return Err(Error::Malformed); }
        while r.has_remaining() {
            if r.remaining() < 4 { return Err(Error::Malformed); }
            let id = r.get_u16::<BigEndian>();
            let len = r.get_u16::<BigEndian>();
            if r.remaining() < len as usize { return Err(Error::Malformed); }
            match id {
                0x0000 => {
                    if len != 4 || initial_max_stream_data { return Err(Error::Malformed); }
                    params.initial_max_stream_data = r.get_u32::<BigEndian>();
                    initial_max_stream_data = true;
                }
                0x0001 => {
                    if len != 4 || initial_max_data { return Err(Error::Malformed); }
                    params.initial_max_data = r.get_u32::<BigEndian>();
                    initial_max_data = true;
                }
                0x0003 => {
                    if len != 2 || idle_timeout { return Err(Error::Malformed); }
                    params.idle_timeout = r.get_u16::<BigEndian>();
                    idle_timeout = true;
                }
                0x0006 => {
                    if len != 16 || params.stateless_reset_token.is_some() { return Err(Error::Malformed); }
                    let mut tok = [0; 16];
                    r.copy_to_slice(&mut tok);
                    params.stateless_reset_token = Some(tok);
                }
                0x0002 => {
                    if len != 2 || initial_max_streams_bidi { return Err(Error::Malformed); }
                    params.initial_max_streams_bidi = r.get_u16::<BigEndian>();
                    initial_max_streams_bidi = true;
                }
                0x0008 => {
                    if len != 2 || initial_max_streams_uni { return Err(Error::Malformed); }
                    params.initial_max_streams_uni = r.get_u16::<BigEndian>();
                    initial_max_streams_uni = true;
                }
                0x0005 => {
                    if len != 2 || params.max_packet_size.is_some() { return Err(Error::Malformed); }
                    params.max_packet_size = Some(r.get_u16::<BigEndian>());
                }
                0x0007 => {
                    if len != 1 || ack_delay_exponent { return Err(Error::Malformed); }
                    params.ack_delay_exponent = r.get_u8();
                    ack_delay_exponent = true;
                    if params.ack_delay_exponent > 20 { return Err(Error::IllegalValue); }
                }
                _ => r.advance(len as usize),
            }
        }

        if initial_max_stream_data && initial_max_data && idle_timeout && (params.stateless_reset_token.is_none() || side == Side::Client) {
            Ok(params)
        } else {
            Err(Error::IllegalValue)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use bytes::IntoBuf;

    #[test]
    fn coding() {
        let mut buf = Vec::new();
        let params = TransportParameters {
            initial_max_streams_bidi: 16,
            initial_max_streams_uni: 16,
            ack_delay_exponent: 2,
            max_packet_size: Some(1200),
            ..TransportParameters::default()
        };
        params.write(&mut buf);
        assert_eq!(TransportParameters::read(Side::Server, &mut buf.into_buf()).unwrap(), params);
    }
}
