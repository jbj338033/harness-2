use crate::streaming::Sink;
use async_trait::async_trait;
use harness_proto::{ErrorCode, ErrorObject, Request, Response};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub type HandlerResult = Result<Value, ErrorObject>;

#[async_trait]
pub trait Handler: Send + Sync {
    async fn call(&self, params: Option<Value>, sink: Sink) -> HandlerResult;
}

#[async_trait]
impl<F, Fut> Handler for F
where
    F: Fn(Option<Value>, Sink) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = HandlerResult> + Send + 'static,
{
    async fn call(&self, params: Option<Value>, sink: Sink) -> HandlerResult {
        self(params, sink).await
    }
}

#[derive(Default, Clone)]
pub struct Router {
    handlers: Arc<HashMap<String, Arc<dyn Handler>>>,
}

impl Router {
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn route(self, method: impl Into<String>, handler: Arc<dyn Handler>) -> Self {
        let mut map = (*self.handlers).clone();
        map.insert(method.into(), handler);
        Self {
            handlers: Arc::new(map),
        }
    }

    pub async fn dispatch(&self, req: Request, sink: Sink) -> Response {
        let Request {
            id, method, params, ..
        } = req;

        let Some(handler) = self.handlers.get(&method) else {
            return Response::err(
                id,
                ErrorObject::new(
                    ErrorCode::MethodNotFound,
                    format!("unknown method: {method}"),
                ),
            );
        };

        match handler.call(params, sink).await {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, e),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

#[must_use]
pub fn discarding_sink() -> (Sink, tokio::sync::mpsc::Receiver<String>) {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    (Sink::new(tx), rx)
}

pub fn parse_params<T: serde::de::DeserializeOwned>(
    params: Option<Value>,
) -> Result<T, ErrorObject> {
    let v = params.unwrap_or(Value::Null);
    serde_json::from_value(v)
        .map_err(|e| ErrorObject::new(ErrorCode::InvalidParams, format!("invalid params: {e}")))
}

#[must_use]
pub fn handler<F, Fut>(f: F) -> Arc<dyn Handler>
where
    F: Fn(Option<Value>, Sink) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = HandlerResult> + Send + 'static,
{
    Arc::new(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_proto::{Id, ResponsePayload};
    use serde_json::json;

    #[tokio::test]
    async fn unknown_method_returns_method_not_found() {
        let router = Router::new();
        let (sink, _rx) = discarding_sink();
        let req = Request::new(Id::Number(1), "missing", None);
        let res = router.dispatch(req, sink).await;
        match res.payload {
            ResponsePayload::Error(e) => assert_eq!(e.code, ErrorCode::MethodNotFound as i32),
            ResponsePayload::Result(_) => panic!("expected error"),
        }
    }

    #[tokio::test]
    async fn ping_roundtrips() {
        let router = Router::new().route(
            "ping",
            handler(|_p, _s| async move { Ok(json!({"pong": true})) }),
        );
        let (sink, _rx) = discarding_sink();
        let req = Request::new(Id::Number(42), "ping", None);
        let res = router.dispatch(req, sink).await;
        match res.payload {
            ResponsePayload::Result(v) => assert_eq!(v, json!({"pong": true})),
            ResponsePayload::Error(_) => panic!("expected result"),
        }
        assert_eq!(res.id, Id::Number(42));
    }

    #[tokio::test]
    async fn handler_streams_notifications() {
        let router = Router::new().route(
            "stream",
            handler(|_p, sink| async move {
                sink.notify("stream.delta", Some(json!({"content": "a"})))
                    .await
                    .unwrap();
                sink.notify("stream.delta", Some(json!({"content": "b"})))
                    .await
                    .unwrap();
                Ok(json!({"ok": true}))
            }),
        );
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let sink = Sink::new(tx);
        let req = Request::new(Id::Number(7), "stream", None);
        drop(router.dispatch(req, sink).await);

        let first = rx.recv().await.unwrap();
        let second = rx.recv().await.unwrap();
        assert!(first.contains("\"content\":\"a\""));
        assert!(second.contains("\"content\":\"b\""));
    }

    #[tokio::test]
    async fn versioned_route_keys_dispatch_literally() {
        let router = Router::new().route(
            "v1.chat.send",
            handler(|_p, _s| async move { Ok(json!({"ok": true})) }),
        );
        let (sink, _rx) = discarding_sink();
        let req = Request::new(Id::Number(1), "v1.chat.send", None);
        let res = router.dispatch(req, sink).await;
        matches!(res.payload, ResponsePayload::Result(_));
    }

    #[tokio::test]
    async fn unversioned_call_to_versioned_route_is_method_not_found() {
        let router = Router::new().route(
            "v1.chat.send",
            handler(|_p, _s| async move { Ok(json!({"ok": true})) }),
        );
        let (sink, _rx) = discarding_sink();
        let req = Request::new(Id::Number(1), "chat.send", None);
        let res = router.dispatch(req, sink).await;
        match res.payload {
            ResponsePayload::Error(e) => assert_eq!(e.code, ErrorCode::MethodNotFound as i32),
            ResponsePayload::Result(_) => {
                panic!("expected MethodNotFound for unversioned call to versioned route")
            }
        }
    }

    #[tokio::test]
    async fn invalid_params_surface_code() {
        let router = Router::new().route(
            "add",
            handler(|p, _s| async move {
                #[derive(serde::Deserialize)]
                struct Args {
                    a: i64,
                    b: i64,
                }
                let args: Args = parse_params(p)?;
                Ok(json!(args.a + args.b))
            }),
        );
        let (sink, _rx) = discarding_sink();
        let req = Request::new(Id::Number(1), "add", Some(json!({"a": "not a number"})));
        let res = router.dispatch(req, sink).await;
        match res.payload {
            ResponsePayload::Error(e) => assert_eq!(e.code, ErrorCode::InvalidParams as i32),
            ResponsePayload::Result(_) => panic!("expected error"),
        }
    }
}
