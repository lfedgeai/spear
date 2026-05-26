use crate::spearlet::execution::host_api::errno::SPEAR_EINVAL;

pub(crate) fn parse_ssf_v1_header(frame: &[u8]) -> Result<(u32, u16), i32> {
    let hdr = spear_ssf::parse_v1_header(frame).map_err(|_| -SPEAR_EINVAL)?;
    Ok((hdr.stream_id, hdr.msg_type))
}

#[cfg(test)]
pub(crate) fn build_ssf_v1_frame(
    stream_id: u32,
    msg_type: u16,
    meta: &[u8],
    data: &[u8],
) -> Vec<u8> {
    spear_ssf::build_v1_frame(stream_id, msg_type, 0, 1, meta, data)
}
