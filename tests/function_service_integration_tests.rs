use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::time::sleep;
use tonic::transport::Server;

use spear_next::proto::spearlet::{
    execution_service_client::ExecutionServiceClient,
    execution_service_server::ExecutionServiceServer,
    invocation_service_client::InvocationServiceClient,
    invocation_service_server::InvocationServiceServer, GetExecutionRequest, InvokeRequest,
    TerminateExecutionRequest,
};
use spear_next::spearlet::{FunctionServiceImpl, SpearletConfig};

async fn start_test_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let svc = FunctionServiceImpl::new(Arc::new(SpearletConfig::default()), None)
        .await
        .unwrap();
    let invocation = InvocationServiceServer::new(svc.clone());
    let execution = ExecutionServiceServer::new(svc);

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(invocation)
            .add_service(execution)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    sleep(Duration::from_millis(100)).await;
    (addr, handle)
}

async fn invocation_client(addr: SocketAddr) -> InvocationServiceClient<tonic::transport::Channel> {
    InvocationServiceClient::connect(format!("http://{}", addr))
        .await
        .unwrap()
}

async fn execution_client(addr: SocketAddr) -> ExecutionServiceClient<tonic::transport::Channel> {
    ExecutionServiceClient::connect(format!("http://{}", addr))
        .await
        .unwrap()
}

#[tokio::test]
async fn test_invoke_requires_task_id() {
    let (addr, _handle) = start_test_server().await;
    let mut client = invocation_client(addr).await;

    let err = client
        .invoke(tonic::Request::new(InvokeRequest::default()))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_get_execution_not_found() {
    let (addr, _handle) = start_test_server().await;
    let mut client = execution_client(addr).await;

    let err = client
        .get_execution(tonic::Request::new(GetExecutionRequest {
            execution_id: "does-not-exist".to_string(),
            include_output: true,
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_terminate_execution_not_found() {
    let (addr, _handle) = start_test_server().await;
    let mut client = execution_client(addr).await;

    let err = client
        .terminate_execution(tonic::Request::new(TerminateExecutionRequest {
            execution_id: "any".to_string(),
            reason: "test".to_string(),
        }))
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

// Additional integration tests would go here...
// 其他集成测试将在这里...
