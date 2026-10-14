use tokio::task::JoinHandle;

#[derive(Debug)]
pub struct TaskGuard(JoinHandle<()>);

impl TaskGuard {
    pub fn spawn<F>(future: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self(tokio::spawn(future))
    }

    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
