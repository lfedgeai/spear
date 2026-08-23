use crate::{constants, SpearError};

pub use spear_ssf::{
    HeaderV1 as SsfV1Header, MsgType as SsfMsgType, SSF_HEADER_LEN_V1, SSF_MAGIC, SSF_VERSION_V1,
};

fn invalid_frame(op: &'static str) -> SpearError {
    SpearError {
        code: "invalid_ssf_frame",
        errno: -constants::SPEAR_EINVAL,
        op,
    }
}

pub fn parse_ssf_v1_header(frame: &[u8]) -> Result<SsfV1Header, SpearError> {
    spear_ssf::parse_v1_header(frame).map_err(|_| invalid_frame("ssf_v1_parse_header"))
}

pub fn build_ssf_v1_frame(stream_id: u32, msg_type: u16, meta: &[u8], data: &[u8]) -> Vec<u8> {
    spear_ssf::build_v1_frame(stream_id, msg_type, 0, 1, meta, data)
}
