pub mod exec;
pub mod failure;
pub mod forward;
pub mod logs;

use std::sync::{Arc, Mutex};

use fleet_core::action::Notice;
use fleet_core::{
    ClusterId, Command, ExecRequest, Forward, LogRequest, Receiver, ResourceRef, Sender, Snapshot,
    Update,
};

use crate::error::{Error, Result};
use crate::manager::{ClusterHandle, lock};
use failure::action_failure;

struct Active {
    forward: Forward,
    handle: ClusterHandle,
    id: u64,
}

impl Active {
    fn stop(&self) {
        self.handle.stop_port_forward(self.id);
    }

    fn alive(&self) -> bool {
        self.handle
            .port_forwards()
            .iter()
            .any(|info| info.id == self.id)
    }
}

#[derive(Default)]
struct Registry {
    forwards: Vec<Active>,
}

#[derive(Clone)]
pub struct Actions {
    registry: Arc<Mutex<Registry>>,
    updates: Sender<Update>,
    changed: Sender<()>,
}

pub fn cluster_of(command: &Command) -> Option<&ClusterId> {
    match command {
        Command::StreamLogs(request) => Some(&request.pod.cluster),
        Command::Exec(request) => Some(&request.target.pod.cluster),
        Command::PortForward { service, .. } => Some(&service.cluster),
        _ => None,
    }
}

fn failed_title(verb: &str, target: &ResourceRef) -> String {
    let kind = fleet_core::resource::short_kind(&target.kind);
    format!("{verb} {kind}/{} failed", target.name)
}

fn failed_detail(target: &ResourceRef, error: &Error) -> String {
    format!("{} · {}", target.cluster, action_failure(error).line())
}

impl Actions {
    pub fn new(updates: Sender<Update>) -> (Self, Receiver<()>) {
        let (changed, signals) = async_channel::unbounded();
        let actions = Self {
            registry: Arc::default(),
            updates,
            changed,
        };
        (actions, signals)
    }

    fn notify(&self, notice: Notice) {
        if self.updates.try_send(Update::Notice(notice)).is_err() {
            tracing::warn!("UI stopped accepting notices");
        }
    }

    fn touch(&self) {
        self.changed.try_send(()).ok();
    }

    pub fn run(&self, command: &Command, handle: Option<ClusterHandle>) {
        match command {
            Command::StopPortForward { local } => self.stop_forward(*local),
            Command::StreamLogs(request) => Self::stream_logs(request, handle),
            Command::Exec(request) => Self::exec(request, handle),
            Command::PortForward {
                service,
                local,
                remote,
            } => {
                let forward = Forward {
                    target: service.clone(),
                    local: *local,
                    remote: *remote,
                };
                self.start_forward(forward, handle);
            }
            _ => {}
        }
    }

    fn stream_logs(request: &LogRequest, handle: Option<ClusterHandle>) {
        if let Some(handle) = handle {
            tokio::spawn(logs::stream(handle, request.clone()));
            return;
        }
        let line = logs::marker(&Error::NotConnected.to_string());
        request.sink.send_blocking(line);
    }

    fn exec(request: &ExecRequest, handle: Option<ClusterHandle>) {
        if let Some(handle) = handle {
            tokio::spawn(exec::run(handle, request.clone()));
            return;
        }
        exec::fail(&request.output, &Error::NotConnected.to_string());
    }

    fn start_forward(&self, forward: Forward, handle: Option<ClusterHandle>) {
        let this = self.clone();
        tokio::spawn(async move {
            let started = match handle {
                Some(handle) => this.forward(forward.clone(), handle).await,
                None => Err(Error::NotConnected),
            };
            if let Err(error) = started {
                let title = failed_title("Port-forward", &forward.target);
                this.notify(Notice::failed(
                    title,
                    failed_detail(&forward.target, &error),
                ));
            }
        });
    }

    async fn forward(&self, forward: Forward, handle: ClusterHandle) -> Result<()> {
        let info = forward::start(&handle, &forward.target, forward.local, forward.remote).await?;
        let active = Active {
            forward: Forward {
                local: info.local_addr.port(),
                ..forward
            },
            handle,
            id: info.id,
        };
        lock(&self.registry).forwards.push(active);
        self.touch();
        Ok(())
    }

    fn stop_forward(&self, local: u16) {
        let mut registry = lock(&self.registry);
        registry.forwards.retain(|active| {
            let keep = active.forward.local != local;
            if !keep {
                active.stop();
            }
            keep
        });
        drop(registry);
        self.touch();
    }

    pub fn overlay(&self, snapshot: &mut Snapshot) {
        let mut registry = lock(&self.registry);
        registry.forwards.retain(|active| {
            let connected = snapshot.cluster(&active.forward.target.cluster).is_some();
            let keep = connected && active.alive();
            if !keep {
                active.stop();
            }
            keep
        });
        snapshot.forwards = registry
            .forwards
            .iter()
            .map(|active| active.forward.clone())
            .collect();
    }
}
