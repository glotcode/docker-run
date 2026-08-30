use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::docker_run::cleanup;
use crate::docker_run::docker;
use crate::docker_run::unix_stream;

const CONTAINER_NAME_PREFIX: &str = "docker-run-prewarm-";
const MAX_CONSECUTIVE_FAILURES: u32 = 5;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
const CIRCUIT_COOLDOWN: Duration = Duration::from_secs(60);
const CIRCUIT_SCAN_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct Config {
    pub images: Vec<String>,
    pub size_per_image: usize,
    pub worker_threads: usize,
    pub io_timeout: Duration,
}

#[derive(Clone)]
pub struct Handle {
    inner: Arc<Inner>,
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handle")
            .field("enabled", &self.is_enabled())
            .finish_non_exhaustive()
    }
}

pub enum Claim {
    NotConfigured,
    Miss,
    Hit(Lease),
}

pub struct Lease {
    container_id: String,
    container_name: Option<String>,
    inner: Arc<Inner>,
}

impl Lease {
    pub fn container_id(&self) -> &str {
        &self.container_id
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(container_name) = self.container_name.take() {
            self.inner.cleanup.schedule(container_name);
        }

        let mut state = lock_or_recover(&self.inner.state, "prewarm pool state");
        state.active_leases = state.active_leases.saturating_sub(1);
        let shutdown_can_continue =
            state.lifecycle == Lifecycle::ShuttingDown && state.active_leases == 0;
        drop(state);
        if shutdown_can_continue {
            self.inner.state_changed.notify_all();
        }
    }
}

struct Inner {
    size_per_image: usize,
    stream_config: unix_stream::Config,
    cleanup: cleanup::Handle,
    state: Mutex<State>,
    state_changed: Condvar,
    sender: Mutex<Option<mpsc::Sender<Job>>>,
    workers: Mutex<Vec<thread::JoinHandle<()>>>,
    container_sequence: AtomicU64,
}

struct State {
    pools: HashMap<String, ImagePool>,
    lifecycle: Lifecycle,
    active_leases: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Lifecycle {
    Running,
    ShuttingDown,
    Stopped,
}

struct ImagePool {
    container_config: docker::ContainerConfig,
    available: Vec<WarmContainer>,
    creating: usize,
    consecutive_failures: u32,
    circuit_open: bool,
    recovery_probe: bool,
    last_failure: Option<Instant>,
}

struct WarmContainer {
    id: String,
    name: String,
}

struct Job {
    image: String,
    container_config: docker::ContainerConfig,
}

pub fn start(
    mut stream_config: unix_stream::Config,
    config: Config,
    container_configs: Vec<docker::ContainerConfig>,
    cleanup: cleanup::Handle,
) -> Result<Handle, Error> {
    stream_config.read_timeout = config.io_timeout;
    stream_config.write_timeout = config.io_timeout;

    let removed = cleanup::remove_containers_with_prefix(&stream_config, CONTAINER_NAME_PREFIX)
        .map_err(Error::Cleanup)?;

    if removed > 0 {
        log::info!("Removed {removed} prewarmed containers left by a previous process");
    }

    let (sender, receiver) = mpsc::channel();
    let inner = Arc::new(Inner {
        size_per_image: config.size_per_image,
        stream_config,
        cleanup,
        state: Mutex::new(State {
            pools: HashMap::new(),
            lifecycle: Lifecycle::Running,
            active_leases: 0,
        }),
        state_changed: Condvar::new(),
        sender: Mutex::new(Some(sender)),
        workers: Mutex::new(Vec::new()),
        container_sequence: AtomicU64::new(0),
    });
    let handle = Handle {
        inner: Arc::clone(&inner),
    };

    if config.size_per_image == 0 || config.images.is_empty() {
        return Ok(handle);
    }

    let receiver = Arc::new(Mutex::new(receiver));
    for worker_id in 0..config.worker_threads.max(1) {
        let worker_inner = Arc::clone(&inner);
        let worker_receiver = Arc::clone(&receiver);
        let worker = match thread::Builder::new()
            .name(format!("container-prewarm-{worker_id}"))
            .spawn(move || worker(worker_inner, worker_receiver))
        {
            Ok(worker) => worker,
            Err(err) => {
                handle.shutdown();
                return Err(Error::StartWorker(err));
            }
        };
        lock_or_recover(&inner.workers, "prewarm worker list").push(worker);
    }

    let recovery_inner = Arc::clone(&inner);
    let recovery_worker = match thread::Builder::new()
        .name("container-prewarm-circuit-recovery".to_string())
        .spawn(move || circuit_recovery_worker(recovery_inner))
    {
        Ok(worker) => worker,
        Err(err) => {
            handle.shutdown();
            return Err(Error::StartWorker(err));
        }
    };
    lock_or_recover(&inner.workers, "prewarm worker list").push(recovery_worker);

    let configured_images: HashSet<_> = config.images.into_iter().collect();
    for container_config in container_configs {
        if configured_images.contains(&container_config.image) {
            handle.add_pool(container_config);
        }
    }

    Ok(handle)
}

impl Handle {
    pub fn claim(&self, image: &str) -> Claim {
        let container = {
            let mut state = lock_or_recover(&self.inner.state, "prewarm pool state");
            if state.lifecycle != Lifecycle::Running {
                return Claim::NotConfigured;
            }
            let Some(pool) = state.pools.get_mut(image) else {
                return Claim::NotConfigured;
            };

            let container = pool.available.pop();
            if container.is_some() {
                state.active_leases = state.active_leases.saturating_add(1);
            }
            container
        };

        let Some(container) = container else {
            return Claim::Miss;
        };

        self.inner.schedule_fill(image);

        Claim::Hit(Lease {
            container_id: container.id,
            container_name: Some(container.name),
            inner: Arc::clone(&self.inner),
        })
    }

    pub fn shutdown(&self) {
        {
            let mut state = lock_or_recover(&self.inner.state, "prewarm pool state");
            match state.lifecycle {
                Lifecycle::Stopped => return,
                Lifecycle::ShuttingDown => {
                    while state.lifecycle != Lifecycle::Stopped {
                        state =
                            wait_or_recover(&self.inner.state_changed, state, "prewarm pool state");
                    }
                    return;
                }
                Lifecycle::Running => state.lifecycle = Lifecycle::ShuttingDown,
            }
        }
        self.inner.state_changed.notify_all();

        log::info!("Shutting down prewarmed container pool");
        lock_or_recover(&self.inner.sender, "prewarm job sender").take();

        let workers = std::mem::take(&mut *lock_or_recover(
            &self.inner.workers,
            "prewarm worker list",
        ));
        for worker in workers {
            if worker.join().is_err() {
                log::error!("Prewarm worker panicked while shutting down");
            }
        }

        let mut state = lock_or_recover(&self.inner.state, "prewarm pool state");
        while state.active_leases > 0 {
            state = wait_or_recover(&self.inner.state_changed, state, "prewarm active leases");
        }
        drop(state);

        match cleanup::remove_containers_with_prefix(
            &self.inner.stream_config,
            CONTAINER_NAME_PREFIX,
        ) {
            Ok(removed) => log::info!("Removed {removed} prewarmed containers during shutdown"),
            Err(err) => log::error!("Failed to remove prewarmed containers during shutdown: {err}"),
        }

        let mut state = lock_or_recover(&self.inner.state, "prewarm pool state");
        state.pools.clear();
        state.lifecycle = Lifecycle::Stopped;
        self.inner.state_changed.notify_all();
    }

    fn is_enabled(&self) -> bool {
        !lock_or_recover(&self.inner.state, "prewarm pool state")
            .pools
            .is_empty()
    }

    fn add_pool(&self, container_config: docker::ContainerConfig) {
        let image = container_config.image.clone();
        lock_or_recover(&self.inner.state, "prewarm pool state")
            .pools
            .entry(image.clone())
            .or_insert(ImagePool {
                container_config,
                available: Vec::new(),
                creating: 0,
                consecutive_failures: 0,
                circuit_open: false,
                recovery_probe: false,
                last_failure: None,
            });
        self.inner.schedule_fill(&image);
    }
}

impl Inner {
    fn schedule_fill(&self, image: &str) {
        let jobs = {
            let mut state = lock_or_recover(&self.state, "prewarm pool state");
            if state.lifecycle != Lifecycle::Running {
                return;
            }
            let Some(pool) = state.pools.get_mut(image) else {
                return;
            };
            if pool.circuit_open {
                return;
            }
            let target_size = if pool.recovery_probe {
                1
            } else {
                self.size_per_image
            };
            let desired =
                target_size.saturating_sub(pool.available.len().saturating_add(pool.creating));
            pool.creating = pool.creating.saturating_add(desired);

            (0..desired)
                .map(|_| Job {
                    image: image.to_string(),
                    container_config: pool.container_config.clone(),
                })
                .collect::<Vec<_>>()
        };

        if jobs.is_empty() {
            return;
        }

        let sender = lock_or_recover(&self.sender, "prewarm job sender");
        let mut unsent = 0;
        for job in jobs {
            if sender
                .as_ref()
                .is_none_or(|sender| sender.send(job).is_err())
            {
                unsent += 1;
            }
        }
        drop(sender);

        if unsent > 0 {
            let mut state = lock_or_recover(&self.state, "prewarm pool state");
            if let Some(pool) = state.pools.get_mut(image) {
                pool.creating = pool.creating.saturating_sub(unsent);
            }
        }
    }

    fn next_container_name(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let sequence = self.container_sequence.fetch_add(1, Ordering::Relaxed);

        format!(
            "{CONTAINER_NAME_PREFIX}{}-{timestamp}-{sequence}",
            std::process::id()
        )
    }

    fn create_container(
        &self,
        config: &docker::ContainerConfig,
    ) -> Result<WarmContainer, WorkerError> {
        let name = self.next_container_name();
        let create_result =
            unix_stream::with_stream(&self.stream_config, WorkerError::UnixStream, |stream| {
                docker::create_container(stream, config, &name).map_err(WorkerError::Create)
            });

        let response = match create_result {
            Ok(response) => response,
            Err(err) => {
                if err.create_is_ambiguous()
                    && let Err(cleanup_err) =
                        cleanup::remove_with_retry(&self.stream_config, &name, true)
                {
                    log::error!(
                        "Failed to resolve ambiguous prewarm create for {name}: {cleanup_err}"
                    );
                }
                return Err(err);
            }
        };
        let id = response.into_body().id;

        let start_result =
            unix_stream::with_stream(&self.stream_config, WorkerError::UnixStream, |stream| {
                docker::start_container(stream, &id).map_err(WorkerError::Start)
            });

        if let Err(err) = start_result {
            if let Err(cleanup_err) = cleanup::remove_with_retry(&self.stream_config, &name, false)
            {
                log::error!("Failed to remove prewarm container {name}: {cleanup_err}");
            }
            return Err(err);
        }

        Ok(WarmContainer { id, name })
    }

    fn finish_job(&self, image: &str, result: Result<WarmContainer, WorkerError>) {
        let mut container_to_remove = None;
        let mut retry_delay = None;
        let mut refill_now = false;

        {
            let mut state = lock_or_recover(&self.state, "prewarm pool state");
            let lifecycle = state.lifecycle;
            if let Some(pool) = state.pools.get_mut(image) {
                pool.creating = pool.creating.saturating_sub(1);
                match result {
                    Ok(container) if lifecycle != Lifecycle::Running => {
                        container_to_remove = Some(container.name);
                    }
                    Ok(container) => {
                        log::debug!("Prewarmed container {} for image {image}", container.id);
                        pool.consecutive_failures = 0;
                        pool.circuit_open = false;
                        pool.recovery_probe = false;
                        pool.last_failure = None;
                        pool.available.push(container);
                        refill_now = true;
                    }
                    Err(_) if lifecycle == Lifecycle::Running => {
                        retry_delay = record_failure(pool);
                        if pool.circuit_open {
                            log::error!(
                                "Opening prewarm circuit for image {image} after {} consecutive failures",
                                pool.consecutive_failures
                            );
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        if let Some(name) = container_to_remove {
            self.cleanup.schedule(name);
        }

        if refill_now {
            self.schedule_fill(image);
        }

        if let Some(delay) = retry_delay
            && self.wait_for_retry(delay)
        {
            self.schedule_fill(image);
        }
    }

    fn is_running(&self) -> bool {
        lock_or_recover(&self.state, "prewarm pool state").lifecycle == Lifecycle::Running
    }

    fn wait_for_retry(&self, delay: Duration) -> bool {
        let deadline = Instant::now() + delay;
        let mut state = lock_or_recover(&self.state, "prewarm pool state");

        loop {
            if state.lifecycle != Lifecycle::Running {
                return false;
            }
            let now = Instant::now();
            if now >= deadline {
                return true;
            }
            (state, _) = wait_timeout_or_recover(
                &self.state_changed,
                state,
                deadline.duration_since(now),
                "prewarm pool state",
            );
        }
    }
}

fn record_failure(pool: &mut ImagePool) -> Option<Duration> {
    pool.consecutive_failures = pool.consecutive_failures.saturating_add(1);
    pool.last_failure = Some(Instant::now());

    if pool.recovery_probe || pool.consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
        pool.circuit_open = true;
        pool.recovery_probe = false;
        None
    } else {
        Some(retry_delay_for(pool.consecutive_failures))
    }
}

fn retry_delay_for(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(5);
    Duration::from_secs(1_u64 << exponent).min(MAX_RETRY_DELAY)
}

fn circuit_recovery_worker(inner: Arc<Inner>) {
    loop {
        let state = lock_or_recover(&inner.state, "prewarm pool state");
        if state.lifecycle != Lifecycle::Running {
            return;
        }
        let (mut state, _) = wait_timeout_or_recover(
            &inner.state_changed,
            state,
            CIRCUIT_SCAN_INTERVAL,
            "prewarm pool state",
        );
        if state.lifecycle != Lifecycle::Running {
            return;
        }

        let now = Instant::now();
        let images = state
            .pools
            .iter_mut()
            .filter_map(|(image, pool)| {
                let ready_for_probe = pool.circuit_open
                    && pool
                        .last_failure
                        .is_some_and(|failed_at| now.duration_since(failed_at) >= CIRCUIT_COOLDOWN);
                if ready_for_probe {
                    pool.circuit_open = false;
                    pool.recovery_probe = true;
                    Some(image.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        drop(state);

        for image in images {
            log::info!("Half-opening prewarm circuit for image {image}");
            inner.schedule_fill(&image);
        }
    }
}

fn worker(inner: Arc<Inner>, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) {
    loop {
        let job = match lock_or_recover(&receiver, "prewarm job receiver").recv() {
            Ok(job) => job,
            Err(_) => return,
        };

        if !inner.is_running() {
            inner.finish_job(&job.image, Err(WorkerError::ShuttingDown));
            continue;
        }

        let result = inner.create_container(&job.container_config);
        if let Err(err) = &result {
            log::error!("Failed to prewarm image {}: {err}", job.image);
        }
        inner.finish_job(&job.image, result);
    }
}

fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, description: &str) -> MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|err| {
        log::error!("Recovering poisoned {description} lock");
        err.into_inner()
    })
}

fn wait_or_recover<'a, T>(
    condvar: &Condvar,
    guard: MutexGuard<'a, T>,
    description: &str,
) -> MutexGuard<'a, T> {
    condvar.wait(guard).unwrap_or_else(|err| {
        log::error!("Recovering poisoned {description} lock after wait");
        err.into_inner()
    })
}

fn wait_timeout_or_recover<'a, T>(
    condvar: &Condvar,
    guard: MutexGuard<'a, T>,
    timeout: Duration,
    description: &str,
) -> (MutexGuard<'a, T>, std::sync::WaitTimeoutResult) {
    condvar.wait_timeout(guard, timeout).unwrap_or_else(|err| {
        log::error!("Recovering poisoned {description} lock after timed wait");
        err.into_inner()
    })
}

#[derive(Debug)]
pub enum Error {
    Cleanup(cleanup::CleanupError),
    StartWorker(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cleanup(err) => write!(f, "Failed to clean old prewarmed containers: {err}"),
            Error::StartWorker(err) => write!(f, "Failed to start prewarm worker: {err}"),
        }
    }
}

#[derive(Debug)]
enum WorkerError {
    UnixStream(unix_stream::Error),
    Create(docker::Error),
    Start(docker::Error),
    ShuttingDown,
}

impl WorkerError {
    fn create_is_ambiguous(&self) -> bool {
        matches!(self, WorkerError::Create(err) if docker::create_error_is_ambiguous(err))
    }
}

impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkerError::UnixStream(err) => write!(f, "Docker socket error: {err}"),
            WorkerError::Create(err) => write!(f, "Container create failed: {err}"),
            WorkerError::Start(err) => write!(f, "Container start failed: {err}"),
            WorkerError::ShuttingDown => write!(f, "Pool is shutting down"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    #[derive(Clone)]
    struct FakeContainer {
        id: String,
        name: String,
        state: String,
    }

    struct FakeDockerState {
        containers: Vec<FakeContainer>,
        requests: Vec<String>,
        next_id: usize,
    }

    #[test]
    fn recovers_fills_claims_and_removes_pool_containers() {
        let _ = env_logger::builder().is_test(true).try_init();
        let socket_path = unique_socket_path();
        let state = Arc::new(Mutex::new(FakeDockerState {
            containers: vec![FakeContainer {
                id: "stale".to_string(),
                name: format!("/{CONTAINER_NAME_PREFIX}stale"),
                state: "running".to_string(),
            }],
            requests: Vec::new(),
            next_id: 1,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let server = start_fake_docker(&socket_path, Arc::clone(&state), Arc::clone(&stop));

        let socket_config = unix_stream::Config {
            path: socket_path.clone(),
            read_timeout: Duration::from_secs(2),
            write_timeout: Duration::from_secs(2),
        };
        let cleanup_handle = cleanup::start(
            socket_config.clone(),
            cleanup::Config {
                worker_threads: 1,
                io_timeout: Duration::from_secs(2),
                recover_stale: false,
            },
        );
        let image = "glot/python:test".to_string();
        let pool = start(
            socket_config,
            Config {
                images: vec![image.clone()],
                size_per_image: 1,
                worker_threads: 1,
                io_timeout: Duration::from_secs(2),
            },
            vec![container_config(image.clone())],
            cleanup_handle,
        )
        .expect("pool should start");

        let lease = wait_for_claim(&pool, &image);
        assert_eq!(lease.container_id(), "warm-1");
        assert_eq!(
            lock_or_recover(&pool.inner.state, "prewarm pool state").active_leases,
            1
        );

        let shutdown_pool = pool.clone();
        let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel();
        let shutdown_thread = thread::spawn(move || {
            shutdown_pool.shutdown();
            shutdown_complete_tx
                .send(())
                .expect("shutdown completion should send");
        });
        wait_until("shutdown transition", Duration::from_secs(2), || {
            lock_or_recover(&pool.inner.state, "prewarm pool state").lifecycle
                == Lifecycle::ShuttingDown
        });
        assert!(matches!(pool.claim(&image), Claim::NotConfigured));
        assert!(matches!(
            shutdown_complete_rx.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        drop(lease);
        shutdown_complete_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("shutdown should finish after the lease is released");
        shutdown_thread
            .join()
            .expect("shutdown thread should finish");
        wait_until("shutdown cleanup", Duration::from_secs(2), || {
            lock_or_recover(&state, "fake Docker state")
                .containers
                .is_empty()
        });

        let requests = lock_or_recover(&state, "fake Docker state")
            .requests
            .clone();
        assert!(
            requests
                .iter()
                .any(|request| request == "DELETE /containers/stale?v=1&force=1")
        );
        assert!(requests.iter().any(|request| {
            request.starts_with("POST /containers/create?name=docker-run-prewarm-")
        }));
        assert!(
            requests
                .iter()
                .any(|request| request == "POST /containers/warm-1/start")
        );

        stop.store(true, Ordering::Release);
        let _ = UnixStream::connect(&socket_path);
        server.join().expect("fake Docker server should stop");
        let _ = std::fs::remove_file(socket_path);
    }

    #[test]
    fn failures_back_off_and_open_the_circuit() {
        let mut image_pool = ImagePool {
            container_config: container_config("glot/python:test".to_string()),
            available: Vec::new(),
            creating: 0,
            consecutive_failures: 0,
            circuit_open: false,
            recovery_probe: false,
            last_failure: None,
        };

        for expected_seconds in [1, 2, 4, 8] {
            assert_eq!(
                record_failure(&mut image_pool),
                Some(Duration::from_secs(expected_seconds))
            );
            assert!(!image_pool.circuit_open);
        }

        assert_eq!(record_failure(&mut image_pool), None);
        assert!(image_pool.circuit_open);
        assert_eq!(retry_delay_for(u32::MAX), MAX_RETRY_DELAY);
    }

    #[test]
    fn failed_half_open_probe_immediately_reopens_the_circuit() {
        let mut image_pool = ImagePool {
            container_config: container_config("glot/python:test".to_string()),
            available: Vec::new(),
            creating: 0,
            consecutive_failures: 0,
            circuit_open: false,
            recovery_probe: true,
            last_failure: None,
        };

        assert_eq!(record_failure(&mut image_pool), None);
        assert!(image_pool.circuit_open);
        assert!(!image_pool.recovery_probe);
    }

    fn wait_for_claim(pool: &Handle, image: &str) -> Lease {
        let mut lease = None;
        wait_until("pool fill", Duration::from_secs(2), || {
            match pool.claim(image) {
                Claim::Hit(claimed) => {
                    lease = Some(claimed);
                    true
                }
                Claim::Miss => false,
                Claim::NotConfigured => panic!("image should have a configured pool"),
            }
        });
        lease.expect("container should become available")
    }

    fn wait_until(description: &str, timeout: Duration, mut predicate: impl FnMut() -> bool) {
        let started_at = Instant::now();
        while !predicate() {
            assert!(
                started_at.elapsed() < timeout,
                "timed out waiting for {description}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn container_config(image: String) -> docker::ContainerConfig {
        docker::ContainerConfig {
            hostname: "glot".to_string(),
            user: "glot".to_string(),
            attach_stdin: true,
            attach_stdout: true,
            attach_stderr: true,
            tty: false,
            open_stdin: true,
            stdin_once: true,
            image,
            network_disabled: true,
            host_config: docker::HostConfig {
                memory: 1_000_000,
                privileged: false,
                cap_add: vec![],
                cap_drop: vec![],
                ulimits: vec![],
                readonly_rootfs: true,
                tmpfs: HashMap::new(),
            },
        }
    }

    fn unique_socket_path() -> PathBuf {
        std::env::current_dir()
            .expect("test working directory should exist")
            .join(format!(
                "docker-run-warm-pool-test-{}-{}.sock",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ))
    }

    fn start_fake_docker(
        socket_path: &PathBuf,
        state: Arc<Mutex<FakeDockerState>>,
        stop: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path).expect("fake Docker socket should bind");
        thread::spawn(move || {
            for connection in listener.incoming() {
                if stop.load(Ordering::Acquire) {
                    return;
                }
                let mut connection = connection.expect("fake Docker connection should succeed");
                handle_fake_request(&mut connection, &state);
            }
        })
    }

    fn handle_fake_request(stream: &mut UnixStream, state: &Mutex<FakeDockerState>) {
        let mut reader = BufReader::new(stream.try_clone().expect("stream should clone"));
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .expect("request line should read");
        let request = request_line
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ");

        let mut content_length = 0;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).expect("header should read");
            if header == "\r\n" || header.is_empty() {
                break;
            }
            if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().expect("content length should parse");
            }
        }
        let mut body = vec![0; content_length];
        reader
            .read_exact(&mut body)
            .expect("request body should read");

        let (status, response_body) = fake_response(&request, state);
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("response should write");
    }

    fn fake_response(request: &str, state: &Mutex<FakeDockerState>) -> (&'static str, String) {
        let mut state = lock_or_recover(state, "fake Docker state");
        state.requests.push(request.to_string());

        if request == "GET /containers/json?all=1" {
            let body = state
                .containers
                .iter()
                .map(|container| {
                    serde_json::json!({
                        "Id": container.id,
                        "Names": [container.name],
                        "State": container.state,
                        "Created": 1
                    })
                })
                .collect::<Vec<_>>();
            return ("200 OK", serde_json::to_string(&body).unwrap());
        }

        if let Some(name) = request.strip_prefix("POST /containers/create?name=") {
            let id = format!("warm-{}", state.next_id);
            state.next_id += 1;
            state.containers.push(FakeContainer {
                id: id.clone(),
                name: format!("/{name}"),
                state: "created".to_string(),
            });
            return (
                "201 Created",
                serde_json::json!({"Id": id, "Warnings": []}).to_string(),
            );
        }

        if let Some(id) = request
            .strip_prefix("POST /containers/")
            .and_then(|request| request.strip_suffix("/start"))
            && let Some(container) = state.containers.iter_mut().find(|item| item.id == id)
        {
            container.state = "running".to_string();
            return ("204 No Content", String::new());
        }

        if let Some(id) = request
            .strip_prefix("DELETE /containers/")
            .and_then(|request| request.strip_suffix("?v=1&force=1"))
        {
            let original_len = state.containers.len();
            state.containers.retain(|container| container.id != id);
            return if state.containers.len() < original_len {
                ("204 No Content", String::new())
            } else {
                ("404 Not Found", String::new())
            };
        }

        ("404 Not Found", String::new())
    }
}
