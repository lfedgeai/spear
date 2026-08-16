//! SMS service implementations
//! SMS服务实现

pub mod node_service;
pub mod resource_service;
pub mod task_assignment_service;
pub mod task_service;

#[cfg(test)]
pub mod test_utils;

pub use node_service::NodeService;
pub use resource_service::ResourceService;
pub use task_assignment_service::TaskAssignmentService;
pub use task_service::TaskService;

pub mod error {
    //! Error types for SMS services / SMS服务的错误类型

    use thiserror::Error;

    /// SMS service error types / SMS服务错误类型
    #[derive(Error, Debug)]
    pub enum SmsError {
        /// Database error / 数据库错误
        #[error("Database error: {0}")]
        Database(String),

        /// Network error / 网络错误
        #[error("Network error: {0}")]
        Network(String),

        /// Configuration error / 配置错误
        #[error("Configuration error: {0}")]
        Config(String),

        /// Service unavailable / 服务不可用
        #[error("Service unavailable: {0}")]
        ServiceUnavailable(String),

        /// Invalid request / 无效请求
        #[error("Invalid request: {0}")]
        InvalidRequest(String),

        /// Conflict / 冲突
        #[error("Conflict: {0}")]
        Conflict(String),

        /// Serialization error / 序列化错误
        #[error("Serialization error: {0}")]
        Serialization(String),

        /// Not found error / 未找到错误
        #[error("Not found: {0}")]
        NotFound(String),
    }

    /// Result type for SMS operations / SMS操作的结果类型
    pub type SmsResult<T> = Result<T, SmsError>;

    /// Convert SmsError to tonic::Status / 将SmsError转换为tonic::Status
    impl From<SmsError> for tonic::Status {
        fn from(err: SmsError) -> Self {
            match err {
                SmsError::Database(msg) => {
                    tonic::Status::internal(format!("Database error: {}", msg))
                }
                SmsError::Network(msg) => {
                    tonic::Status::unavailable(format!("Network error: {}", msg))
                }
                SmsError::Config(msg) => {
                    tonic::Status::invalid_argument(format!("Configuration error: {}", msg))
                }
                SmsError::ServiceUnavailable(msg) => {
                    tonic::Status::unavailable(format!("Service unavailable: {}", msg))
                }
                SmsError::InvalidRequest(msg) => {
                    tonic::Status::invalid_argument(format!("Invalid request: {}", msg))
                }
                SmsError::Conflict(msg) => {
                    tonic::Status::already_exists(format!("Conflict: {}", msg))
                }
                SmsError::Serialization(msg) => {
                    tonic::Status::internal(format!("Serialization error: {}", msg))
                }
                SmsError::NotFound(msg) => tonic::Status::not_found(format!("Not found: {}", msg)),
            }
        }
    }
}
