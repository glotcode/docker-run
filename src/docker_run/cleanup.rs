use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::docker_run::docker;
use crate::docker_run::unix_stream;

const CONTAINER_NAME_PREFIX: &str = "docker-run-";
const MAX_ATTEMPTS: usize = 5;
const RETRY_DELAY: Duration = Duration::from_secs(1);
const RECOVERY_INTERVAL: Duration = Duration::from_secs(300);
const STALE_CREATED_AGE: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct Config {
    pub worker_threads: usize,
    pub io_timeout: Duration,
    pub recover_stale: bool,
}

#[derive(Clone, Debug)]
pub struct Handle {
    sender: mpsc::Sender<Job>,
}

#[derive(Debug)]
enum Job {
    Remove {
        target: String,
        wait_for_creation: bool,
    },
    Recover,
}

pub fn start(mut stream_config: unix_stream::Config, config: Config) -> Handle {
    stream_config.read_timeout = config.io_timeout;
    stream_config.write_timeout = config.io_timeout;

    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));
    let handle = Handle { sender };

    for worker_id in 0..config.worker_threads.max(1) {
        let worker_receiver = Arc::clone(&receiver);
        let worker_config = stream_config.clone();
        let worker_handle = handle.clone();

        thread::Builder::new()
            .name(format!("container-cleanup-{worker_id}"))
            .spawn(move || worker(worker_receiver, worker_config, worker_handle))
            .expect("failed to start container cleanup worker");
    }

    if config.recover_stale {
        let recovery_handle = handle.clone();
        thread::Builder::new()
            .name("container-cleanup-recovery".to_string())
            .spawn(move || {
                loop {
                    recovery_handle.send(Job::Recover);
                    thread::sleep(RECOVERY_INTERVAL);
                }
            })
            .expect("failed to start container cleanup recovery worker");
    }

    handle
}

impl Handle {
    pub fn schedule(&self, target: String) {
        self.send(Job::Remove {
            target,
            wait_for_creation: false,
        });
    }

    pub fn schedule_after_ambiguous_create(&self, target: String) {
        self.send(Job::Remove {
            target,
            wait_for_creation: true,
        });
    }

    fn send(&self, job: Job) {
        if let Err(err) = self.sender.send(job) {
            log::error!("Failed to queue container cleanup: {err}");
        }
    }
}

fn worker(
    receiver: Arc<Mutex<mpsc::Receiver<Job>>>,
    stream_config: unix_stream::Config,
    handle: Handle,
) {
    loop {
        let job = match receiver.lock() {
            Ok(receiver) => receiver.recv(),
            Err(err) => {
                log::error!("Container cleanup queue lock poisoned: {err}");
                return;
            }
        };

        match job {
            Ok(Job::Remove {
                target,
                wait_for_creation,
            }) => remove_with_retry(&stream_config, &target, wait_for_creation),
            Ok(Job::Recover) => recover_created_containers(&stream_config, &handle),
            Err(_) => return,
        }
    }
}

fn remove_with_retry(stream_config: &unix_stream::Config, target: &str, wait_for_creation: bool) {
    for attempt in 1..=MAX_ATTEMPTS {
        let result = unix_stream::with_stream(stream_config, CleanupError::UnixStream, |stream| {
            docker::remove_container(stream, target).map_err(CleanupError::Docker)
        });

        match result {
            Ok(_) => return,
            Err(err)
                if docker_error_is_not_found(&err)
                    && (!wait_for_creation || attempt == MAX_ATTEMPTS) =>
            {
                return;
            }
            Err(err) if attempt < MAX_ATTEMPTS => {
                log::warn!(
                    "Container cleanup attempt {attempt}/{MAX_ATTEMPTS} failed for {target}: {err}"
                );
                thread::sleep(RETRY_DELAY);
            }
            Err(err) => {
                log::error!(
                    "Container cleanup failed after {MAX_ATTEMPTS} attempts for {target}: {err}"
                );
            }
        }
    }
}

fn docker_error_is_not_found(err: &CleanupError) -> bool {
    matches!(err, CleanupError::Docker(err) if docker::is_not_found(err))
}

fn recover_created_containers(stream_config: &unix_stream::Config, handle: &Handle) {
    let result = unix_stream::with_stream(stream_config, CleanupError::UnixStream, |stream| {
        docker::list_containers(stream).map_err(CleanupError::Docker)
    });

    let containers = match result {
        Ok(response) => response.into_body(),
        Err(err) => {
            log::error!("Failed to find stale docker-run containers: {err}");
            return;
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for container in containers {
        if is_stale_managed_container(&container, now) {
            log::info!("Scheduling stale container {} for cleanup", container.id);
            handle.schedule(container.id);
        }
    }
}

fn is_stale_managed_container(container: &docker::ContainerListItem, now: i64) -> bool {
    let is_managed = container.names.iter().any(|name| {
        name.strip_prefix('/')
            .is_some_and(|name| name.starts_with(CONTAINER_NAME_PREFIX))
    });
    let age = now.saturating_sub(container.created);

    is_managed && container.state == "created" && age >= STALE_CREATED_AGE.as_secs() as i64
}

#[derive(Debug)]
enum CleanupError {
    UnixStream(unix_stream::Error),
    Docker(docker::Error),
}

impl std::fmt::Display for CleanupError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            CleanupError::UnixStream(err) => write!(f, "{err}"),
            CleanupError::Docker(err) => write!(f, "{err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(name: &str, state: &str, created: i64) -> docker::ContainerListItem {
        docker::ContainerListItem {
            id: "container-id".to_string(),
            names: vec![name.to_string()],
            state: state.to_string(),
            created,
        }
    }

    #[test]
    fn recovery_only_selects_stale_created_managed_containers() {
        assert!(is_stale_managed_container(
            &container("/docker-run-1", "created", 100),
            160
        ));
        assert!(!is_stale_managed_container(
            &container("/docker-run-1", "created", 101),
            160
        ));
        assert!(!is_stale_managed_container(
            &container("/docker-run-1", "running", 100),
            160
        ));
        assert!(!is_stale_managed_container(
            &container("/someone-else", "created", 100),
            160
        ));
    }
}
