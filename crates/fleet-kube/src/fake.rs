use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use http::{Method, Request, Response, StatusCode};
use kube::Client;
use kube::client::Body;
use serde_json::Value;
use tower_service::Service;

use crate::discovery::{ContextRef, KubeContext};
use crate::manager::{ClusterHandle, ClusterManager, lock};
use crate::provider::{AuthKind, Provider};
use crate::settings::KubeSettings;
use crate::shell_env::ShellEnv;

const NAMESPACE: &str = "default";
const MISSING_KUBECONFIG: &str = "/missing-fake-kubeconfig";
const NOT_FOUND: &str =
    r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"NotFound","code":404}"#;

pub type Reply = (StatusCode, String);
type Respond = dyn Fn(&Method, &str, &Value) -> Option<Reply> + Send + Sync;

#[derive(Default)]
pub struct Traffic {
    active: AtomicUsize,
    peak: AtomicUsize,
    requests: Mutex<Vec<(Method, String)>>,
    bodies: Mutex<Vec<(Method, String, Value)>>,
}

impl Traffic {
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    pub fn requests(&self) -> Vec<(Method, String)> {
        lock(&self.requests).clone()
    }

    pub fn bodies(&self, method: &Method, path: &str) -> Vec<Value> {
        lock(&self.bodies)
            .iter()
            .filter(|(sent, at, _)| sent == method && at.starts_with(path))
            .map(|(_, _, body)| body.clone())
            .collect()
    }
}

#[derive(Clone)]
struct Fake {
    delay: Duration,
    respond: Arc<Respond>,
    traffic: Arc<Traffic>,
}

impl Service<Request<Body>> for Fake {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let method = request.method().clone();
        let path = request.uri().to_string();
        lock(&self.traffic.requests).push((method.clone(), path.clone()));
        let delay = self.delay;
        let traffic = self.traffic.clone();
        let respond = self.respond.clone();
        Box::pin(async move {
            let bytes = request
                .into_body()
                .collect_bytes()
                .await
                .unwrap_or_default();
            let sent = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            let (status, body) = respond(&method, &path, &sent)
                .unwrap_or_else(|| (StatusCode::NOT_FOUND, NOT_FOUND.to_owned()));
            lock(&traffic.bodies).push((method, path, sent));
            let now = traffic.active.fetch_add(1, Ordering::SeqCst) + 1;
            traffic.peak.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            traffic.active.fetch_sub(1, Ordering::SeqCst);
            let mut response = Response::new(Body::from(body.into_bytes()));
            *response.status_mut() = status;
            response.headers_mut().insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
            Ok(response)
        })
    }
}

pub fn client(
    delay: Duration,
    respond: impl Fn(&Method, &str) -> Option<Reply> + Send + Sync + 'static,
) -> (Client, Arc<Traffic>) {
    client_reading_bodies(delay, move |method, path, _| respond(method, path))
}

pub fn client_reading_bodies(
    delay: Duration,
    respond: impl Fn(&Method, &str, &Value) -> Option<Reply> + Send + Sync + 'static,
) -> (Client, Arc<Traffic>) {
    let traffic = Arc::new(Traffic::default());
    let fake = Fake {
        delay,
        respond: Arc::new(respond),
        traffic: traffic.clone(),
    };
    (Client::new(fake, NAMESPACE), traffic)
}

pub fn handle(client: &Client) -> (ClusterManager, ClusterHandle) {
    let settings = KubeSettings::default();
    let env = Arc::new(ShellEnv::from_process(String::new()));
    let (manager, _) = ClusterManager::new(tokio::runtime::Handle::current(), settings, env);
    let context = KubeContext {
        id: ContextRef {
            file: MISSING_KUBECONFIG.into(),
            context: NAMESPACE.to_owned(),
        },
        cluster: NAMESPACE.to_owned(),
        user: None,
        namespace: None,
        server: None,
        provider: Provider::Generic,
        auth: AuthKind::None,
        is_current: false,
    };
    let handle = manager.connect(context);
    handle.shared.set_client(Some(client));
    (manager, handle)
}
