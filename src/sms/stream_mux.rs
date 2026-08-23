use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Mutex;

#[derive(Clone, Eq)]
pub struct ClientStreamKey {
    pub client_id: String,
    pub stream_id: u32,
}

impl PartialEq for ClientStreamKey {
    fn eq(&self, other: &Self) -> bool {
        self.client_id == other.client_id && self.stream_id == other.stream_id
    }
}

impl Hash for ClientStreamKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.client_id.hash(state);
        self.stream_id.hash(state);
    }
}

#[derive(Default)]
struct RouterInner {
    client_to_up: HashMap<ClientStreamKey, u32>,
    up_to_client: HashMap<u32, (String, u32)>,
}

pub struct ExecutionStreamRouter {
    next_up_stream_id: AtomicU32,
    inner: std::sync::Arc<Mutex<RouterInner>>,
}

impl Clone for ExecutionStreamRouter {
    fn clone(&self) -> Self {
        Self {
            next_up_stream_id: AtomicU32::new(self.next_up_stream_id.load(Ordering::Relaxed)),
            inner: self.inner.clone(),
        }
    }
}

impl Default for ExecutionStreamRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ExecutionStreamRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecutionStreamRouter")
            .field(
                "next_up_stream_id",
                &self.next_up_stream_id.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl ExecutionStreamRouter {
    pub fn new() -> Self {
        Self {
            next_up_stream_id: AtomicU32::new(1),
            inner: std::sync::Arc::new(Mutex::new(RouterInner::default())),
        }
    }

    pub async fn route_client_to_upstream(
        &self,
        client_id: &str,
        frame: &[u8],
    ) -> Result<Vec<u8>, String> {
        let (client_stream_id, _msg_type) = parse_ssf_v1_header(frame)?;
        let key = ClientStreamKey {
            client_id: client_id.to_string(),
            stream_id: client_stream_id,
        };
        let mut inner = self.inner.lock().await;
        let up_stream_id = match inner.client_to_up.get(&key).copied() {
            Some(v) => v,
            None => {
                // Prefer the original client stream id when it is free so single-client
                // user-stream workloads keep their expected stream semantics through the
                // endpoint gateway. When there is a collision, fall back to remapping.
                // 优先保留原始 client stream id，这样单客户端 user-stream 任务在
                // endpoint gateway 下仍能保持预期的流语义；只有冲突时才回退到重写。
                let preferred_stream_id = if client_stream_id == 0 {
                    None
                } else if inner.up_to_client.contains_key(&client_stream_id) {
                    None
                } else {
                    Some(client_stream_id)
                };
                let v = self.alloc_up_stream_id(&inner, preferred_stream_id);
                if v == 0 {
                    return Err("no available upstream stream_id".to_string());
                }
                inner.client_to_up.insert(key.clone(), v);
                inner
                    .up_to_client
                    .insert(v, (key.client_id.clone(), key.stream_id));
                v
            }
        };
        drop(inner);
        let mut out = frame.to_vec();
        write_stream_id(&mut out, up_stream_id)?;
        Ok(out)
    }

    pub async fn route_upstream_to_client(
        &self,
        frame: &[u8],
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        let (up_stream_id, _msg_type) = parse_ssf_v1_header(frame)?;
        let inner = self.inner.lock().await;
        let Some((client_id, client_stream_id)) = inner.up_to_client.get(&up_stream_id).cloned()
        else {
            return Ok(None);
        };
        drop(inner);
        let mut out = frame.to_vec();
        write_stream_id(&mut out, client_stream_id)?;
        Ok(Some((client_id, out)))
    }

    pub async fn remove_client(&self, client_id: &str) {
        let mut inner = self.inner.lock().await;
        let mut to_remove = Vec::new();
        for (k, up) in inner.client_to_up.iter() {
            if k.client_id == client_id {
                to_remove.push((k.clone(), *up));
            }
        }
        for (k, up) in to_remove {
            inner.client_to_up.remove(&k);
            inner.up_to_client.remove(&up);
        }
    }

    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.client_to_up.is_empty() && inner.up_to_client.is_empty()
    }

    fn alloc_up_stream_id(&self, inner: &RouterInner, preferred_stream_id: Option<u32>) -> u32 {
        if let Some(preferred_stream_id) = preferred_stream_id {
            if !inner.up_to_client.contains_key(&preferred_stream_id) {
                return preferred_stream_id;
            }
        }
        for _ in 0..1024 {
            let id = self.next_up_stream_id.fetch_add(1, Ordering::Relaxed);
            let id = if id == 0 { 1 } else { id };
            if !inner.up_to_client.contains_key(&id) {
                return id;
            }
        }
        0
    }
}

const SSF_MAGIC: [u8; 4] = *b"SPST";
const SSF_VERSION_V1: u16 = 1;
const SSF_HEADER_MIN: usize = 32;

pub fn parse_ssf_v1_header(frame: &[u8]) -> Result<(u32, u16), String> {
    if frame.len() < SSF_HEADER_MIN {
        return Err("ssf frame too short".to_string());
    }
    if frame[0..4] != SSF_MAGIC {
        return Err("ssf bad magic".to_string());
    }
    let version = u16::from_le_bytes([frame[4], frame[5]]);
    if version != SSF_VERSION_V1 {
        return Err("ssf bad version".to_string());
    }
    let header_len = u16::from_le_bytes([frame[6], frame[7]]) as usize;
    if header_len < SSF_HEADER_MIN || frame.len() < header_len {
        return Err("ssf bad header_len".to_string());
    }
    let stream_id = u32::from_le_bytes([frame[12], frame[13], frame[14], frame[15]]);
    let meta_len = u32::from_le_bytes([frame[24], frame[25], frame[26], frame[27]]) as usize;
    let data_len = u32::from_le_bytes([frame[28], frame[29], frame[30], frame[31]]) as usize;
    let remain = frame.len().saturating_sub(header_len);
    if meta_len.saturating_add(data_len) != remain {
        return Err("ssf len mismatch".to_string());
    }
    let msg_type = u16::from_le_bytes([frame[8], frame[9]]);
    Ok((stream_id, msg_type))
}

pub fn write_stream_id(frame: &mut [u8], stream_id: u32) -> Result<(), String> {
    if frame.len() < SSF_HEADER_MIN {
        return Err("ssf frame too short".to_string());
    }
    frame[12..16].copy_from_slice(&stream_id.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_frame(stream_id: u32, msg_type: u16, meta: &[u8], data: &[u8]) -> Vec<u8> {
        let header_len: u16 = SSF_HEADER_MIN as u16;
        let mut out = Vec::with_capacity(header_len as usize + meta.len() + data.len());
        out.extend_from_slice(&SSF_MAGIC);
        out.extend_from_slice(&SSF_VERSION_V1.to_le_bytes());
        out.extend_from_slice(&header_len.to_le_bytes());
        out.extend_from_slice(&msg_type.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&stream_id.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&(meta.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(meta);
        out.extend_from_slice(data);
        out
    }

    #[tokio::test]
    async fn two_clients_same_stream_id_do_not_collide() {
        let r = ExecutionStreamRouter::new();
        let f = build_frame(1, 2, b"{}", b"hello");
        let a = r.route_client_to_upstream("c1", &f).await.unwrap();
        let b = r.route_client_to_upstream("c2", &f).await.unwrap();
        let (sa, _) = parse_ssf_v1_header(&a).unwrap();
        let (sb, _) = parse_ssf_v1_header(&b).unwrap();
        assert_eq!(sa, 1);
        assert_ne!(sa, sb);

        let back_a = r.route_upstream_to_client(&a).await.unwrap().unwrap();
        let back_b = r.route_upstream_to_client(&b).await.unwrap().unwrap();
        assert_eq!(back_a.0, "c1");
        assert_eq!(back_b.0, "c2");
        let (csa, _) = parse_ssf_v1_header(&back_a.1).unwrap();
        let (csb, _) = parse_ssf_v1_header(&back_b.1).unwrap();
        assert_eq!(csa, 1);
        assert_eq!(csb, 1);
    }

    #[tokio::test]
    async fn first_client_keeps_original_stream_id_when_available() {
        let r = ExecutionStreamRouter::new();
        let f = build_frame(2, 2, b"{}", b"voice");
        let upstream = r.route_client_to_upstream("c1", &f).await.unwrap();
        let (stream_id, _) = parse_ssf_v1_header(&upstream).unwrap();
        assert_eq!(stream_id, 2);
    }

    #[tokio::test]
    async fn disconnect_cleans_mappings() {
        let r = ExecutionStreamRouter::new();
        let f = build_frame(1, 2, b"{}", b"hello");
        let _ = r.route_client_to_upstream("c1", &f).await.unwrap();
        assert!(!r.is_empty().await);
        r.remove_client("c1").await;
        assert!(r.is_empty().await);
    }
}
