/*
 * 模块概述
 * ========
 * API多路复用模块是ChirpStack LoRaWAN网络服务器的关键基础设施组件，负责处理不同类型的HTTP请求，
 * 特别是区分和路由gRPC请求与其他HTTP请求。该模块使ChirpStack能够在同一端口上同时提供gRPC API
 * 和其他HTTP服务（如REST API或Web界面），提高了系统的灵活性和资源利用率。
 * 
 * 在现代微服务架构中，支持多种协议是常见需求。本模块通过Tower中间件框架实现了请求多路复用，
 * 使ChirpStack能够无缝集成gRPC和HTTP服务，同时保持代码的模块化和可维护性。
 *
 * 文件功能
 * ========
 * 本文件(grpc_multiplex.rs)实现了基于Tower服务模型的gRPC请求多路复用机制。
 * 它提供了一个中间件层，能够检查传入的HTTP请求，判断是否为gRPC请求（基于Content-Type头），
 * 并将请求路由到适当的处理服务。这使得ChirpStack可以在单一HTTP服务器实例上同时提供gRPC API
 * 和其他HTTP服务。
 *
 * 主要组件
 * ========
 * - GrpcMultiplexService: 核心服务结构体，包含gRPC服务和其他HTTP服务的引用
 * - GrpcMultiplexFuture: 表示多路复用服务的异步操作结果
 * - GrpcMultiplexFutureEnum: 内部枚举，区分gRPC和其他HTTP请求的处理Future
 * - GrpcMultiplexLayer: Tower层实现，用于将多路复用服务集成到服务栈中
 * - is_grpc_request: 辅助函数，用于检测请求是否为gRPC请求
 *
 * 关键流程
 * ========
 * 1. 服务器接收到HTTP请求
 * 2. GrpcMultiplexService通过检查Content-Type头判断请求类型
 * 3. 如果Content-Type以"application/grpc"开头，请求被路由到gRPC服务
 * 4. 否则，请求被路由到其他HTTP服务（如REST API）
 * 5. 服务处理请求并生成响应
 * 6. 响应通过多路复用Future返回给客户端
 *
 * 注意事项
 * ========
 * - 多路复用服务需要确保两个底层服务都已准备好接收请求（poll_ready）
 * - 错误处理需要统一，将不同服务的错误类型转换为通用的Box<dyn Error>
 * - 性能考虑：请求分发逻辑应尽可能高效，避免成为瓶颈
 * - 内存安全：使用pin_project宏确保Future的正确固定，避免内存安全问题
 * - 服务状态：需要正确跟踪底层服务的就绪状态，避免向未就绪的服务发送请求
 * - 扩展性：设计允许未来添加更多协议类型的支持
 */

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::ready;
use http::{header::CONTENT_TYPE, Request, Response};
use http_body::Body;
use pin_project::pin_project;
use tower::{Layer, Service};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[pin_project(project = GrpcMultiplexFutureEnumProj)]
enum GrpcMultiplexFutureEnum<FS, FO> {
    Grpc {
        #[pin]
        future: FS,
    },
    Other {
        #[pin]
        future: FO,
    },
}

#[pin_project]
pub struct GrpcMultiplexFuture<FS, FO> {
    #[pin]
    future: GrpcMultiplexFutureEnum<FS, FO>,
}

impl<ResBody, FS, FO, ES, EO> Future for GrpcMultiplexFuture<FS, FO>
where
    ResBody: Body,
    FS: Future<Output = Result<Response<ResBody>, ES>>,
    FO: Future<Output = Result<Response<ResBody>, EO>>,
    ES: Into<BoxError> + Send,
    EO: Into<BoxError> + Send,
{
    type Output = Result<Response<ResBody>, Box<dyn std::error::Error + Send + Sync + 'static>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.future.project() {
            GrpcMultiplexFutureEnumProj::Grpc { future } => future.poll(cx).map_err(Into::into),
            GrpcMultiplexFutureEnumProj::Other { future } => future.poll(cx).map_err(Into::into),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GrpcMultiplexService<S, O> {
    grpc: S,
    other: O,
    grpc_ready: bool,
    other_ready: bool,
}

impl<ReqBody, ResBody, S, O> Service<Request<ReqBody>> for GrpcMultiplexService<S, O>
where
    ResBody: Body,
    S: Service<Request<ReqBody>, Response = Response<ResBody>>,
    O: Service<Request<ReqBody>, Response = Response<ResBody>>,
    S::Error: Into<BoxError> + Send,
    O::Error: Into<BoxError> + Send,
{
    type Response = S::Response;
    type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
    type Future = GrpcMultiplexFuture<S::Future, O::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        loop {
            match (self.grpc_ready, self.other_ready) {
                (true, true) => {
                    return Ok(()).into();
                }
                (false, _) => {
                    ready!(self.grpc.poll_ready(cx)).map_err(Into::into)?;
                    self.grpc_ready = true;
                }
                (_, false) => {
                    ready!(self.other.poll_ready(cx)).map_err(Into::into)?;
                    self.other_ready = true;
                }
            }
        }
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        assert!(self.grpc_ready);
        assert!(self.other_ready);

        if is_grpc_request(&request) {
            GrpcMultiplexFuture {
                future: GrpcMultiplexFutureEnum::Grpc {
                    future: self.grpc.call(request),
                },
            }
        } else {
            GrpcMultiplexFuture {
                future: GrpcMultiplexFutureEnum::Other {
                    future: self.other.call(request),
                },
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GrpcMultiplexLayer<O> {
    other: O,
}

impl<O> GrpcMultiplexLayer<O> {
    pub fn new(other: O) -> Self {
        Self { other }
    }
}

impl<S, O> Layer<S> for GrpcMultiplexLayer<O>
where
    O: Clone,
{
    type Service = GrpcMultiplexService<S, O>;

    fn layer(&self, grpc: S) -> Self::Service {
        GrpcMultiplexService {
            grpc,
            other: self.other.clone(),
            grpc_ready: false,
            other_ready: false,
        }
    }
}

fn is_grpc_request<B>(req: &Request<B>) -> bool {
    req.headers()
        .get(CONTENT_TYPE)
        .map(|content_type| content_type.as_bytes())
        .filter(|content_type| content_type.starts_with(b"application/grpc"))
        .is_some()
}
