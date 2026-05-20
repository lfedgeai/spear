// 连接管理器 / Connection Manager
// 负责管理 spearlet 与 agent 之间的连接 / Manages connections between spearlet and agent

use crate::spearlet::execution::communication::protocol::*;
use crate::spearlet::execution::manager::TaskExecutionManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener as TokioTcpListener, TcpStream as TokioTcpStream};
use tokio::sync::{mpsc, oneshot, Mutex as TokioMutex};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

type ConnectionsMap = Arc<RwLock<HashMap<String, Arc<RwLock<ConnectionState>>>>>;

/// 连接状态 / Connection state
/// 表示单个连接的状态信息 / Represents state information for a single connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionState {
    /// 连接ID / Connection ID
    pub connection_id: String,
    /// 实例ID / Instance ID
    pub instance_id: Option<String>,
    /// 连接地址 / Connection address
    pub remote_addr: SocketAddr,
    /// 连接建立时间 / Connection established time
    #[serde(with = "serde_instant")]
    pub connected_at: Instant,
    /// 最后活跃时间 / Last activity time
    #[serde(with = "serde_instant")]
    pub last_activity: Instant,
    /// 认证状态 / Authentication status
    pub authenticated: bool,
    /// 客户端类型 / Client type
    pub client_type: Option<String>,
    /// 客户端版本 / Client version
    pub client_version: Option<String>,
    /// 会话ID / Session ID
    pub session_id: Option<String>,
    /// 连接状态 / Connection status
    pub status: ConnectionStatus,
    /// 心跳序列号 / Heartbeat sequence number
    pub heartbeat_sequence: u64,
}

/// 连接事件 / Connection event
/// 定义连接生命周期中的各种事件 / Defines various events in connection lifecycle
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// 新连接建立 / New connection established
    Connected {
        connection_id: String,
        remote_addr: SocketAddr,
    },
    /// 连接认证完成 / Connection authenticated
    Authenticated {
        connection_id: String,
        instance_id: String,
        client_type: String,
    },
    /// 连接断开 / Connection disconnected
    Disconnected {
        connection_id: String,
        reason: String,
    },
    /// 消息接收 / Message received
    MessageReceived {
        connection_id: String,
        message: SpearMessage,
    },
    /// 消息发送 / Message sent
    MessageSent {
        connection_id: String,
        message_type: MessageType,
    },
    /// 心跳超时 / Heartbeat timeout
    HeartbeatTimeout { connection_id: String },
    /// 认证失败 / Authentication failed
    AuthenticationFailed {
        connection_id: String,
        reason: String,
    },
    /// 连接错误 / Connection error
    Error {
        connection_id: String,
        error_type: String,
        error_message: String,
    },
}

/// 连接管理器配置 / Connection manager configuration
/// 配置连接管理器的各种参数 / Configures various parameters for connection manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionManagerConfig {
    /// 监听地址 / Listen address
    pub listen_addr: String,
    /// 端口范围 / Port range
    pub port_range: (u16, u16),
    /// 最大连接数 / Maximum connections
    pub max_connections: usize,
    /// 连接超时时间 / Connection timeout
    #[serde(with = "serde_duration_seconds")]
    pub connection_timeout: Duration,
    /// 认证超时时间 / Authentication timeout
    #[serde(with = "serde_duration_seconds")]
    pub auth_timeout: Duration,
    /// 心跳间隔 / Heartbeat interval
    #[serde(with = "serde_duration_seconds")]
    pub heartbeat_interval: Duration,
    /// 心跳超时 / Heartbeat timeout
    #[serde(with = "serde_duration_seconds")]
    pub heartbeat_timeout: Duration,
    /// 最大消息大小 / Maximum message size
    pub max_message_size: usize,
    /// 是否启用TLS / Whether to enable TLS
    pub enable_tls: bool,
    /// TLS证书路径 / TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS私钥路径 / TLS private key path
    pub tls_key_path: Option<String>,
}

mod serde_duration_seconds {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

mod serde_instant {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert Instant to SystemTime for serialization
        // Note: This is an approximation since Instant doesn't have a fixed epoch
        let now_instant = Instant::now();
        let now_system = SystemTime::now();
        let duration_since_now = if *instant > now_instant {
            instant.duration_since(now_instant)
        } else {
            now_instant.duration_since(*instant)
        };

        let system_time = if *instant > now_instant {
            now_system + duration_since_now
        } else {
            now_system - duration_since_now
        };

        let timestamp = system_time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        timestamp.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let timestamp = u64::deserialize(deserializer)?;
        let system_time = UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
        let now_system = SystemTime::now();
        let now_instant = Instant::now();

        // Convert back to Instant (approximation)
        if system_time > now_system {
            let duration_ahead = system_time.duration_since(now_system).unwrap_or_default();
            Ok(now_instant + duration_ahead)
        } else {
            let duration_behind = now_system.duration_since(system_time).unwrap_or_default();
            Ok(now_instant - duration_behind)
        }
    }
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1".to_string(),
            port_range: (
                constants::DEFAULT_PORT_RANGE_START,
                constants::DEFAULT_PORT_RANGE_END,
            ),
            max_connections: 1000,
            connection_timeout: Duration::from_secs(constants::CONNECTION_TIMEOUT_SECS),
            auth_timeout: Duration::from_secs(constants::AUTH_TIMEOUT_SECS),
            heartbeat_interval: Duration::from_secs(constants::HEARTBEAT_INTERVAL_SECS),
            heartbeat_timeout: Duration::from_secs(constants::HEARTBEAT_INTERVAL_SECS * 3),
            max_message_size: constants::MAX_MESSAGE_SIZE,
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

/// 连接处理器 / Connection handler
/// 处理单个连接的所有操作 / Handles all operations for a single connection
struct ConnectionHandler {
    /// 连接ID / Connection ID
    connection_id: String,
    /// TCP流 / TCP stream
    stream: Arc<TokioMutex<TokioTcpStream>>,
    /// 连接状态 / Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// 事件发送器 / Event sender
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// 消息发送器 / Message sender
    #[allow(dead_code)]
    message_sender: mpsc::UnboundedSender<SpearMessage>,
    /// 消息接收器 / Message receiver
    message_receiver: Arc<TokioMutex<mpsc::UnboundedReceiver<SpearMessage>>>,
    /// 关闭信号接收器 / Shutdown signal receiver
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    /// 配置 / Configuration
    config: ConnectionManagerConfig,
}

impl ConnectionHandler {
    /// 创建新的连接处理器 / Create new connection handler
    pub fn new(
        connection_id: String,
        stream: TokioTcpStream,
        remote_addr: SocketAddr,
        event_sender: mpsc::UnboundedSender<ConnectionEvent>,
        shutdown_receiver: oneshot::Receiver<()>,
        config: ConnectionManagerConfig,
    ) -> (Self, mpsc::UnboundedSender<SpearMessage>) {
        let (message_sender, message_receiver) = mpsc::unbounded_channel();

        let state = Arc::new(RwLock::new(ConnectionState {
            connection_id: connection_id.clone(),
            instance_id: None,
            remote_addr,
            connected_at: Instant::now(),
            last_activity: Instant::now(),
            authenticated: false,
            client_type: None,
            client_version: None,
            session_id: None,
            status: ConnectionStatus::Active,
            heartbeat_sequence: 0,
        }));

        let handler = Self {
            connection_id,
            stream: Arc::new(TokioMutex::new(stream)),
            state,
            event_sender,
            message_sender: message_sender.clone(),
            message_receiver: Arc::new(TokioMutex::new(message_receiver)),
            shutdown_receiver: Some(shutdown_receiver),
            config,
        };

        (handler, message_sender)
    }

    /// 运行连接处理器 / Run connection handler
    pub async fn run(mut self) {
        info!("Starting connection handler for {}", self.connection_id);

        // 发送连接建立事件 / Send connection established event
        let _ = self.event_sender.send(ConnectionEvent::Connected {
            connection_id: self.connection_id.clone(),
            remote_addr: self.state.read().unwrap().remote_addr,
        });

        // 将shutdown_receiver移出self以避免借用冲突 / Move shutdown_receiver out of self to avoid borrow conflicts
        let mut shutdown_receiver = self
            .shutdown_receiver
            .take()
            .expect("shutdown_receiver should be available");

        // 启动读取任务 / Start read task
        let read_task = self.start_read_task();

        // 启动写入任务 / Start write task
        let write_task = self.start_write_task();

        // 启动心跳任务 / Start heartbeat task
        let heartbeat_task = self.start_heartbeat_task();

        // 等待任务完成或关闭信号 / Wait for tasks to complete or shutdown signal
        tokio::select! {
            _ = read_task => {
                debug!("Read task completed for {}", self.connection_id);
            }
            _ = write_task => {
                debug!("Write task completed for {}", self.connection_id);
            }
            _ = heartbeat_task => {
                debug!("Heartbeat task completed for {}", self.connection_id);
            }
            _ = &mut shutdown_receiver => {
                info!("Shutdown signal received for {}", self.connection_id);
            }
        }

        // 发送连接断开事件 / Send connection disconnected event
        let _ = self.event_sender.send(ConnectionEvent::Disconnected {
            connection_id: self.connection_id.clone(),
            reason: "Handler shutdown".to_string(),
        });

        info!("Connection handler stopped for {}", self.connection_id);
    }

    /// 启动读取任务 / Start read task
    async fn start_read_task(&self) -> tokio::task::JoinHandle<()> {
        let stream = Arc::clone(&self.stream);
        let state = Arc::clone(&self.state);
        let event_sender = self.event_sender.clone();
        let connection_id = self.connection_id.clone();
        let max_message_size = self.config.max_message_size;

        tokio::spawn(async move {
            let mut buffer = vec![0u8; max_message_size];

            loop {
                let mut stream_guard = stream.lock().await;

                // 读取消息长度 / Read message length
                let mut length_bytes = [0u8; 4];
                match stream_guard.read_exact(&mut length_bytes).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to read message length for {}: {}", connection_id, e);
                        break;
                    }
                }

                let message_length = u32::from_be_bytes(length_bytes) as usize;
                if message_length > max_message_size {
                    error!(
                        "Message too large for {}: {} bytes",
                        connection_id, message_length
                    );
                    break;
                }

                // 读取消息内容 / Read message content
                buffer.resize(message_length, 0);
                match stream_guard.read_exact(&mut buffer).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!(
                            "Failed to read message content for {}: {}",
                            connection_id, e
                        );
                        break;
                    }
                }

                drop(stream_guard);

                // 解析消息 / Parse message
                match SpearMessage::deserialize(&buffer) {
                    Ok(message) => {
                        // 更新最后活跃时间 / Update last activity time
                        {
                            let mut state_guard = state.write().unwrap();
                            state_guard.last_activity = Instant::now();
                        }

                        // 发送消息接收事件 / Send message received event
                        let _ = event_sender.send(ConnectionEvent::MessageReceived {
                            connection_id: connection_id.clone(),
                            message,
                        });
                    }
                    Err(e) => {
                        error!("Failed to deserialize message for {}: {}", connection_id, e);
                        break;
                    }
                }
            }
        })
    }

    /// 启动写入任务 / Start write task
    async fn start_write_task(&self) -> tokio::task::JoinHandle<()> {
        let stream = Arc::clone(&self.stream);
        let message_receiver = Arc::clone(&self.message_receiver);
        let event_sender = self.event_sender.clone();
        let connection_id = self.connection_id.clone();

        tokio::spawn(async move {
            let mut receiver = message_receiver.lock().await;
            while let Some(message) = receiver.recv().await {
                drop(receiver);
                let mut stream_guard = stream.lock().await;

                // 序列化消息 / Serialize message
                let serialized = match message.serialize() {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Failed to serialize message for {}: {}", connection_id, e);
                        receiver = message_receiver.lock().await;
                        continue;
                    }
                };

                // 写入消息长度 / Write message length
                let length_bytes = (serialized.len() as u32).to_be_bytes();
                if let Err(e) = stream_guard.write_all(&length_bytes).await {
                    error!(
                        "Failed to write message length for {}: {}",
                        connection_id, e
                    );
                    break;
                }

                // 写入消息内容 / Write message content
                if let Err(e) = stream_guard.write_all(&serialized).await {
                    error!(
                        "Failed to write message content for {}: {}",
                        connection_id, e
                    );
                    break;
                }

                // 刷新缓冲区 / Flush buffer
                if let Err(e) = stream_guard.flush().await {
                    error!("Failed to flush stream for {}: {}", connection_id, e);
                    break;
                }

                drop(stream_guard);

                // 发送消息发送事件 / Send message sent event
                let _ = event_sender.send(ConnectionEvent::MessageSent {
                    connection_id: connection_id.clone(),
                    message_type: message.message_type,
                });

                receiver = message_receiver.lock().await;
            }
        })
    }

    /// 启动心跳任务 / Start heartbeat task
    async fn start_heartbeat_task(&self) -> tokio::task::JoinHandle<()> {
        let state = Arc::clone(&self.state);
        let event_sender = self.event_sender.clone();
        let connection_id = self.connection_id.clone();
        let heartbeat_timeout = self.config.heartbeat_timeout;

        tokio::spawn(async move {
            let mut interval = interval(heartbeat_timeout);

            loop {
                interval.tick().await;

                let last_activity = {
                    let state_guard = state.read().unwrap();
                    state_guard.last_activity
                };

                if last_activity.elapsed() > heartbeat_timeout {
                    warn!("Heartbeat timeout for connection {}", connection_id);
                    let _ = event_sender.send(ConnectionEvent::HeartbeatTimeout {
                        connection_id: connection_id.clone(),
                    });
                    break;
                }
            }
        })
    }
}

/// Secret 验证回调类型 / Secret validation callback type
pub type SecretValidator = Arc<dyn Fn(&str, &str) -> bool + Send + Sync>;

/// 连接管理器 / Connection Manager
/// 管理所有连接的主要组件 / Main component for managing all connections
pub struct ConnectionManager {
    /// 配置 / Configuration
    config: ConnectionManagerConfig,
    /// 连接状态映射 / Connection state mapping
    connections: Arc<RwLock<HashMap<String, Arc<RwLock<ConnectionState>>>>>,
    /// 实例ID到连接ID的映射 / Instance ID to connection ID mapping
    instance_connections: Arc<RwLock<HashMap<String, String>>>,
    /// 事件发送器 / Event sender
    event_sender: mpsc::UnboundedSender<ConnectionEvent>,
    /// 事件接收器 / Event receiver
    event_receiver: Arc<TokioMutex<mpsc::UnboundedReceiver<ConnectionEvent>>>,
    /// 监听地址 / Listen address
    listen_addr: Arc<RwLock<Option<SocketAddr>>>,
    /// 关闭信号发送器 / Shutdown signal sender
    shutdown_senders: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    /// Secret 验证器 / Secret validator
    secret_validator: Option<SecretValidator>,
    /// 任务执行管理器 / Task execution manager
    execution_manager: Option<Arc<TaskExecutionManager>>,
}

impl ConnectionManager {
    /// 创建新的连接管理器 / Create new connection manager
    pub fn new(config: ConnectionManagerConfig) -> Self {
        Self::new_with_validator(config, None)
    }

    /// 创建带有验证器的连接管理器 / Create connection manager with validator
    pub fn new_with_validator(
        config: ConnectionManagerConfig,
        secret_validator: Option<SecretValidator>,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            instance_connections: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            event_receiver: Arc::new(TokioMutex::new(event_receiver)),
            listen_addr: Arc::new(RwLock::new(None)),
            shutdown_senders: Arc::new(Mutex::new(HashMap::new())),
            secret_validator,
            execution_manager: None,
        }
    }

    /// 创建带有任务执行管理器的连接管理器 / Create connection manager with task execution manager
    pub fn new_with_execution_manager(
        config: ConnectionManagerConfig,
        execution_manager: Arc<TaskExecutionManager>,
    ) -> Self {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            instance_connections: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            event_receiver: Arc::new(TokioMutex::new(event_receiver)),
            listen_addr: Arc::new(RwLock::new(None)),
            shutdown_senders: Arc::new(Mutex::new(HashMap::new())),
            secret_validator: None,
            execution_manager: Some(execution_manager),
        }
    }

    /// 启动连接管理器 / Start connection manager
    pub async fn start(&self) -> Result<SocketAddr, Box<dyn std::error::Error + Send + Sync>> {
        // 查找可用端口 / Find available port
        let listen_addr = self.find_available_port().await?;

        // 绑定监听器 / Bind listener
        let listener = TokioTcpListener::bind(&listen_addr).await?;
        let actual_addr = listener.local_addr()?;

        // 保存监听地址 / Save listen address
        {
            let mut addr_guard = self.listen_addr.write().unwrap();
            *addr_guard = Some(actual_addr);
        }

        info!("Connection manager listening on {}", actual_addr);

        // 启动事件处理任务 / Start event handling task
        self.start_event_handler().await;

        // 启动连接接受任务 / Start connection acceptance task
        self.start_connection_acceptor(listener).await;

        Ok(actual_addr)
    }

    /// 查找可用端口 / Find available port
    async fn find_available_port(
        &self,
    ) -> Result<SocketAddr, Box<dyn std::error::Error + Send + Sync>> {
        let (start_port, end_port) = self.config.port_range;

        for port in start_port..=end_port {
            let addr = format!("{}:{}", self.config.listen_addr, port);
            match TokioTcpListener::bind(&addr).await {
                Ok(_) => {
                    return Ok(addr.parse()?);
                }
                Err(_) => continue,
            }
        }

        Err("No available port found in range".into())
    }

    /// 启动事件处理器 / Start event handler
    async fn start_event_handler(&self) {
        let event_receiver = Arc::clone(&self.event_receiver);
        let connections = Arc::clone(&self.connections);
        let instance_connections = Arc::clone(&self.instance_connections);
        let secret_validator = self.secret_validator.clone();
        let execution_manager = self.execution_manager.clone();

        tokio::spawn(async move {
            let mut receiver = event_receiver.lock().await;

            while let Some(event) = receiver.recv().await {
                match event {
                    ConnectionEvent::Connected {
                        connection_id,
                        remote_addr,
                    } => {
                        info!(
                            "New connection established: {} from {}",
                            connection_id, remote_addr
                        );
                    }

                    ConnectionEvent::Authenticated {
                        connection_id,
                        instance_id,
                        client_type,
                    } => {
                        info!(
                            "Connection authenticated: {} for instance {} ({})",
                            connection_id, instance_id, client_type
                        );

                        // 更新实例连接映射 / Update instance connection mapping
                        {
                            let mut instance_conn_guard = instance_connections.write().unwrap();
                            instance_conn_guard.insert(instance_id.clone(), connection_id.clone());
                        }

                        // 更新连接状态 / Update connection state
                        if let Some(conn_state) = connections.read().unwrap().get(&connection_id) {
                            let mut state_guard = conn_state.write().unwrap();
                            state_guard.authenticated = true;
                            state_guard.instance_id = Some(instance_id);
                            state_guard.client_type = Some(client_type);
                        }
                    }

                    ConnectionEvent::Disconnected {
                        connection_id,
                        reason,
                    } => {
                        info!("Connection disconnected: {} ({})", connection_id, reason);

                        // 清理连接状态 / Clean up connection state
                        let instance_id = {
                            let mut conn_guard = connections.write().unwrap();
                            if let Some(conn_state) = conn_guard.remove(&connection_id) {
                                conn_state.read().unwrap().instance_id.clone()
                            } else {
                                None
                            }
                        };

                        // 清理实例连接映射 / Clean up instance connection mapping
                        if let Some(instance_id) = instance_id {
                            let mut instance_conn_guard = instance_connections.write().unwrap();
                            instance_conn_guard.remove(&instance_id);
                        }
                    }

                    ConnectionEvent::HeartbeatTimeout { connection_id } => {
                        warn!("Heartbeat timeout for connection: {}", connection_id);
                        // TODO: 关闭连接 / Close connection
                    }

                    ConnectionEvent::MessageReceived {
                        connection_id,
                        message,
                    } => {
                        // 处理接收到的消息 / Handle received message
                        match message.message_type {
                            MessageType::AuthRequest => {
                                // 处理身份验证请求 / Handle authentication request
                                if let Err(e) = Self::handle_auth_request_static(
                                    &connection_id,
                                    &message,
                                    &connections,
                                    &instance_connections,
                                    &secret_validator,
                                    &execution_manager,
                                )
                                .await
                                {
                                    error!(
                                        "Failed to handle auth request for {}: {}",
                                        connection_id, e
                                    );
                                    // 发送身份验证失败事件 / Send authentication failed event
                                    // TODO: 实现事件发送逻辑 / Implement event sending logic
                                }
                            }
                            MessageType::Heartbeat => {
                                // 处理心跳消息 / Handle heartbeat message
                                debug!("Received heartbeat from connection: {}", connection_id);
                                // 更新最后活跃时间 / Update last activity time
                                if let Some(conn_state) =
                                    connections.read().unwrap().get(&connection_id)
                                {
                                    let mut state_guard = conn_state.write().unwrap();
                                    state_guard.last_activity = Instant::now();
                                    state_guard.heartbeat_sequence += 1;
                                }
                            }
                            _ => {
                                debug!(
                                    "Received message type {:?} from connection: {}",
                                    message.message_type, connection_id
                                );
                            }
                        }
                    }

                    ConnectionEvent::AuthenticationFailed {
                        connection_id,
                        reason,
                    } => {
                        warn!(
                            "Authentication failed for connection {}: {}",
                            connection_id, reason
                        );
                        // TODO: 关闭连接 / Close connection
                    }

                    _ => {
                        debug!("Received event: {:?}", event);
                    }
                }
            }
        });
    }

    /// 启动连接接受器 / Start connection acceptor
    async fn start_connection_acceptor(&self, listener: TokioTcpListener) {
        let event_sender = self.event_sender.clone();
        let connections = Arc::clone(&self.connections);
        let shutdown_senders = Arc::clone(&self.shutdown_senders);
        let config = self.config.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, remote_addr)) => {
                        let connection_id = Uuid::new_v4().to_string();

                        // 检查连接数限制 / Check connection limit
                        {
                            let conn_guard = connections.read().unwrap();
                            if conn_guard.len() >= config.max_connections {
                                warn!(
                                    "Maximum connections reached, rejecting connection from {}",
                                    remote_addr
                                );
                                continue;
                            }
                        }

                        // 创建关闭信号 / Create shutdown signal
                        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
                        {
                            let mut shutdown_guard = shutdown_senders.lock().unwrap();
                            shutdown_guard.insert(connection_id.clone(), shutdown_sender);
                        }

                        // 创建连接处理器 / Create connection handler
                        let (handler, _message_sender) = ConnectionHandler::new(
                            connection_id.clone(),
                            stream,
                            remote_addr,
                            event_sender.clone(),
                            shutdown_receiver,
                            config.clone(),
                        );

                        // 保存连接状态 / Save connection state
                        {
                            let mut conn_guard = connections.write().unwrap();
                            conn_guard.insert(connection_id.clone(), handler.state.clone());
                        }

                        // 启动连接处理器 / Start connection handler
                        tokio::spawn(handler.run());

                        info!(
                            "Accepted new connection: {} from {}",
                            connection_id, remote_addr
                        );
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }
        });
    }

    /// 获取监听地址 / Get listen address
    pub fn get_listen_addr(&self) -> Option<SocketAddr> {
        *self.listen_addr.read().unwrap()
    }

    /// 获取连接数量 / Get connection count
    pub fn get_connection_count(&self) -> usize {
        self.connections.read().unwrap().len()
    }

    /// 获取所有连接状态 / Get all connection states
    pub fn get_all_connections(&self) -> Vec<ConnectionState> {
        self.connections
            .read()
            .unwrap()
            .values()
            .map(|state| state.read().unwrap().clone())
            .collect()
    }

    /// 根据实例ID获取连接 / Get connection by instance ID
    pub fn get_connection_by_instance(&self, instance_id: &str) -> Option<ConnectionState> {
        let instance_conn_guard = self.instance_connections.read().unwrap();
        if let Some(connection_id) = instance_conn_guard.get(instance_id) {
            let conn_guard = self.connections.read().unwrap();
            if let Some(conn_state) = conn_guard.get(connection_id) {
                return Some(conn_state.read().unwrap().clone());
            }
        }
        None
    }

    /// 关闭指定连接 / Close specific connection
    pub fn close_connection(&self, connection_id: &str) -> Result<(), String> {
        let mut shutdown_guard = self.shutdown_senders.lock().unwrap();
        if let Some(shutdown_sender) = shutdown_guard.remove(connection_id) {
            let _ = shutdown_sender.send(());
            Ok(())
        } else {
            Err(format!("Connection {} not found", connection_id))
        }
    }

    /// 关闭所有连接 / Close all connections
    pub fn close_all_connections(&self) {
        let mut shutdown_guard = self.shutdown_senders.lock().unwrap();
        for (_, shutdown_sender) in shutdown_guard.drain() {
            let _ = shutdown_sender.send(());
        }
    }

    /// 处理身份验证请求 / Handle authentication request
    async fn handle_auth_request_static(
        connection_id: &str,
        message: &SpearMessage,
        connections: &ConnectionsMap,
        instance_connections: &Arc<RwLock<HashMap<String, String>>>,
        secret_validator: &Option<SecretValidator>,
        execution_manager: &Option<Arc<TaskExecutionManager>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // 解析身份验证请求 / Parse authentication request
        let auth_request: AuthRequest = message
            .parse_payload()
            .map_err(|e| format!("Failed to parse auth request: {}", e))?;

        info!(
            "Processing auth request for instance {} from connection {}",
            auth_request.instance_id, connection_id
        );

        // 使用 secret 验证器进行身份验证 / Use secret validator for authentication
        let auth_success = if let Some(manager) = execution_manager {
            // Get secret from TaskInstance
            // 从 TaskInstance 获取 secret
            match manager.get_instance(&auth_request.instance_id) {
                Some(instance) => {
                    let secret_guard = instance.secret.read();
                    match secret_guard.as_ref() {
                        Some(expected_secret) => {
                            let valid = expected_secret == &auth_request.token;
                            debug!(
                                "Secret validation for instance {}: {}",
                                auth_request.instance_id,
                                if valid { "success" } else { "failed" }
                            );
                            valid
                        }
                        None => {
                            warn!(
                                "No secret configured for instance {}",
                                auth_request.instance_id
                            );
                            false
                        }
                    }
                }
                None => {
                    warn!(
                        "Instance {} not found in execution manager",
                        auth_request.instance_id
                    );
                    false
                }
            }
        } else if let Some(validator) = secret_validator {
            // Fallback to legacy validator
            // 回退到传统验证器
            validator(&auth_request.instance_id, &auth_request.token)
        } else {
            warn!("No secret validation configured, rejecting authentication");
            false
        };

        if auth_success {
            // 身份验证成功 / Authentication successful
            info!(
                "Authentication successful for instance {} from connection {}",
                auth_request.instance_id, connection_id
            );

            // 更新实例连接映射 / Update instance connection mapping
            {
                let mut instance_conn_guard = instance_connections.write().unwrap();
                instance_conn_guard
                    .insert(auth_request.instance_id.clone(), connection_id.to_string());
            }

            // 更新连接状态 / Update connection state
            if let Some(conn_state) = connections.read().unwrap().get(connection_id) {
                let mut state_guard = conn_state.write().unwrap();
                state_guard.authenticated = true;
                state_guard.instance_id = Some(auth_request.instance_id.clone());
                state_guard.client_type = Some(auth_request.client_type.clone());
                state_guard.client_version = Some(auth_request.client_version.clone());
            }

            // TODO: 发送身份验证成功响应 / Send authentication success response
            // 创建认证响应 / Create authentication response
            let auth_response = AuthResponse {
                success: true,
                error_message: None,
                session_id: Some(format!("session_{}", connection_id)),
                server_version: "1.0.0".to_string(),
                supported_features: vec!["execute".to_string(), "signal".to_string()],
            };

            // TODO: 发送响应消息 / Send response message
            debug!(
                "Auth response created for connection {}: {:?}",
                connection_id, auth_response
            );
        } else {
            // 身份验证失败 / Authentication failed
            warn!(
                "Authentication failed for instance {} from connection {}: invalid token",
                auth_request.instance_id, connection_id
            );

            // TODO: 发送身份验证失败响应 / Send authentication failure response
            let auth_response = AuthResponse {
                success: false,
                error_message: Some("Invalid authentication token".to_string()),
                session_id: None,
                server_version: "1.0.0".to_string(),
                supported_features: vec![],
            };

            debug!(
                "Auth failure response created for connection {}: {:?}",
                connection_id, auth_response
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_manager_creation() {
        let config = ConnectionManagerConfig::default();
        let manager = ConnectionManager::new(config);

        assert_eq!(manager.get_connection_count(), 0);
        assert!(manager.get_listen_addr().is_none());
    }

    #[tokio::test]
    async fn test_find_available_port() {
        let config = ConnectionManagerConfig {
            port_range: (9100, 9110),
            ..Default::default()
        };
        let manager = ConnectionManager::new(config);

        let addr = manager.find_available_port().await.unwrap();
        assert!(addr.port() >= 9100 && addr.port() <= 9110);
    }

    #[tokio::test]
    async fn test_message_serialization() {
        let auth_req = AuthRequest {
            instance_id: "test-instance".to_string(),
            token: "test-token".to_string(),
            client_version: "1.0.0".to_string(),
            client_type: "process".to_string(),
            extra_params: HashMap::new(),
        };

        let message = SpearMessage::auth_request(123, auth_req).unwrap();
        let serialized = message.serialize().unwrap();
        let deserialized = SpearMessage::deserialize(&serialized).unwrap();

        assert_eq!(message.message_type, deserialized.message_type);
        assert_eq!(message.request_id, deserialized.request_id);
    }
}
