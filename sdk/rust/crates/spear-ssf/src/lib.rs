#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

pub const SSF_MAGIC: [u8; 4] = *b"SPST";
pub const SSF_VERSION_V1: u16 = 1;
pub const SSF_HEADER_LEN_V1: usize = 32;

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgType {
    Ctrl = 1,
    Data = 2,
    Commit = 3,
    Close = 4,
    Error = 6,
}

impl MsgType {
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}

impl TryFrom<u16> for MsgType {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, <Self as TryFrom<u16>>::Error> {
        match value {
            1 => Ok(MsgType::Ctrl),
            2 => Ok(MsgType::Data),
            3 => Ok(MsgType::Commit),
            4 => Ok(MsgType::Close),
            6 => Ok(MsgType::Error),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderV1 {
    pub msg_type: u16,
    pub flags: u16,
    pub stream_id: u32,
    pub seq: u64,
    pub meta_len: u32,
    pub data_len: u32,
    pub header_len: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    TooShort,
    BadMagic,
    BadVersion,
    BadHeaderLen,
    LengthMismatch,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TooShort => write!(f, "frame too short"),
            Error::BadMagic => write!(f, "bad magic"),
            Error::BadVersion => write!(f, "bad version"),
            Error::BadHeaderLen => write!(f, "bad header length"),
            Error::LengthMismatch => write!(f, "length mismatch"),
        }
    }
}

pub fn parse_v1_header(frame: &[u8]) -> Result<HeaderV1, Error> {
    if frame.len() < SSF_HEADER_LEN_V1 {
        return Err(Error::TooShort);
    }
    if frame[0..4] != SSF_MAGIC {
        return Err(Error::BadMagic);
    }
    let version = u16::from_le_bytes([frame[4], frame[5]]);
    if version != SSF_VERSION_V1 {
        return Err(Error::BadVersion);
    }
    let header_len = u16::from_le_bytes([frame[6], frame[7]]);
    let header_len_usize = header_len as usize;
    if header_len_usize < SSF_HEADER_LEN_V1 || frame.len() < header_len_usize {
        return Err(Error::BadHeaderLen);
    }

    let msg_type = u16::from_le_bytes([frame[8], frame[9]]);
    let flags = u16::from_le_bytes([frame[10], frame[11]]);
    let stream_id = u32::from_le_bytes([frame[12], frame[13], frame[14], frame[15]]);
    let seq = u64::from_le_bytes([
        frame[16], frame[17], frame[18], frame[19], frame[20], frame[21], frame[22], frame[23],
    ]);
    let meta_len = u32::from_le_bytes([frame[24], frame[25], frame[26], frame[27]]);
    let data_len = u32::from_le_bytes([frame[28], frame[29], frame[30], frame[31]]);

    let remain = frame.len().saturating_sub(header_len_usize);
    if meta_len.saturating_add(data_len) as usize != remain {
        return Err(Error::LengthMismatch);
    }

    Ok(HeaderV1 {
        msg_type,
        flags,
        stream_id,
        seq,
        meta_len,
        data_len,
        header_len,
    })
}

pub fn split_v1(frame: &[u8]) -> Result<(HeaderV1, &[u8], &[u8]), Error> {
    let hdr = parse_v1_header(frame)?;
    let header_len = hdr.header_len as usize;
    let meta_len = hdr.meta_len as usize;
    let data_len = hdr.data_len as usize;
    let meta_start = header_len;
    let data_start = meta_start + meta_len;
    let data_end = data_start + data_len;
    Ok((
        hdr,
        &frame[meta_start..data_start],
        &frame[data_start..data_end],
    ))
}

pub fn rewrite_stream_id_inplace(frame: &mut [u8], stream_id: u32) -> Result<(), Error> {
    if frame.len() < 16 {
        return Err(Error::TooShort);
    }
    frame[12..16].copy_from_slice(&stream_id.to_le_bytes());
    Ok(())
}

pub fn build_v1_frame(
    stream_id: u32,
    msg_type: u16,
    flags: u16,
    seq: u64,
    meta: &[u8],
    data: &[u8],
) -> Vec<u8> {
    let header_len: u16 = SSF_HEADER_LEN_V1 as u16;
    let mut out = Vec::with_capacity(header_len as usize + meta.len() + data.len());
    out.extend_from_slice(&SSF_MAGIC);
    out.extend_from_slice(&SSF_VERSION_V1.to_le_bytes());
    out.extend_from_slice(&header_len.to_le_bytes());
    out.extend_from_slice(&msg_type.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&stream_id.to_le_bytes());
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(meta);
    out.extend_from_slice(data);
    out
}

pub fn is_close(hdr: &HeaderV1) -> bool {
    hdr.msg_type == MsgType::Close as u16
}

pub fn is_data(hdr: &HeaderV1) -> bool {
    hdr.msg_type == MsgType::Data as u16
}

pub fn is_commit(hdr: &HeaderV1) -> bool {
    hdr.msg_type == MsgType::Commit as u16
}
