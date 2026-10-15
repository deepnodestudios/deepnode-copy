use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use crate::engine::config::TransferConfig;
use crate::engine::item::FileItem;
use crate::engine::stats::TransferStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Canceled,
}

#[derive(Clone)]
pub struct QueueTask {
    pub id: String,
    pub title: String,
    pub config: TransferConfig,
    pub items: Vec<FileItem>,
    pub state: Arc<Mutex<TaskState>>,
    pub pause_flag: Arc<AtomicBool>,
    pub cancel_flag: Arc<AtomicBool>,
    pub stats: Arc<Mutex<TransferStats>>,
}

impl QueueTask {
    pub fn new(id: String, title: String, config: TransferConfig, items: Vec<FileItem>) -> Self {
        Self {
            id,
            title,
            config,
            items,
            state: Arc::new(Mutex::new(TaskState::Queued)),
            pause_flag: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(Mutex::new(TransferStats::default())),
        }
    }

    pub fn pause(&self) {
        self.pause_flag.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.pause_flag.store(false, Ordering::Relaxed);
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
    }
}

pub struct QueueManager {
    tasks: Arc<Mutex<Vec<QueueTask>>>,
    max_concurrent: usize,
}

impl QueueManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            max_concurrent: if max_concurrent == 0 { 1 } else { max_concurrent },
        }
    }

    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    pub async fn add_task(&self, task: QueueTask) {
        let mut tasks = self.tasks.lock().await;
        tasks.push(task);
    }

    pub async fn get_tasks(&self) -> Vec<QueueTask> {
        self.tasks.lock().await.clone()
    }

    /// Processes all queued tasks by dispatching them to the provided executor function.
    pub async fn process_queue<F, Fut>(&self, executor: F)
    where
        F: Fn(QueueTask) -> Fut + Send + Sync + 'static + Clone,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut handles = Vec::new();
        loop {
            let next_task = {
                let mut tasks = self.tasks.lock().await;
                let mut found = None;
                for t in tasks.iter_mut() {
                    let mut s = t.state.lock().await;
                    if *s == TaskState::Queued {
                        *s = TaskState::Running;
                        found = Some(t.clone());
                        break;
                    }
                }
                found
            };

            match next_task {
                Some(task) => {
                    let exec = executor.clone();
                    let t_clone = task.clone();
                    let handle = tokio::spawn(async move {
                        exec(t_clone.clone()).await;
                        let mut s = t_clone.state.lock().await;
                        if *s == TaskState::Running {
                            *s = TaskState::Completed;
                        }
                    });
                    handles.push(handle);
                }
                None => break,
            }
        }

        for handle in handles {
            let _ = handle.await;
        }
    }
}
