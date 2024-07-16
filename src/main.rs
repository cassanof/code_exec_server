use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use lazy_static::lazy_static;
use parquet::{file::reader::FileReader, record::RowAccessor};
use std::{
    collections::HashMap,
    process::Output,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::{io::AsyncReadExt, sync::RwLock, time::Instant};
use tokio::{io::AsyncWriteExt, sync::Mutex};

macro_rules! debug {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            println!($($arg)*);
        }
    };
}

lazy_static! {
    static ref FILE_IDX: AtomicUsize = AtomicUsize::new(0);
    static ref CPUS_AVAILABLE: usize = std::thread::available_parallelism().unwrap().into();
    static ref CRATE_DIR: String =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    // this is total ram / cpu count. this is in kilobytes
    static ref MEMORY_LIMIT: usize = {
        let mem = sys_info::mem_info().unwrap().total as usize;
        let cpus = *CPUS_AVAILABLE;
        mem / cpus
    };
    // this keeps track of pids and when they started, and the timeout allowed
    static ref PID_POOL: Mutex<Vec<(u32, Instant, Duration)>> = Mutex::new(Vec::new());
    // this is how often the GC checks for pids to kill
    static ref GC_INTERVAL: Duration = Duration::from_secs(10);
    // this is a map of test bank names to test bank structs
    static ref TEST_BANKS: Mutex<HashMap<String, TestBank>> = Mutex::new(HashMap::new());
    // max 1 hour of oldness per test bank
    static ref MAX_TEST_BANK_OLDNESS: Duration = Duration::from_secs(3600);
}

async fn garbage_collector() {
    loop {
        tokio::time::sleep(*GC_INTERVAL).await;
        let mut pool = PID_POOL.lock().await;
        let now = Instant::now();
        // get all pids to kill
        let to_kill = pool
            .iter()
            .filter_map(|(pid, start, timeout)| {
                if now.duration_since(*start) > *timeout {
                    Some(*pid)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        // remove them from the pool
        pool.retain(|(pid, _, _)| !to_kill.contains(pid));
        // drop the lock
        drop(pool);
        // kill the pids...
        for pid in to_kill {
            // check if the pid is still alive
            let wpid = nix::sys::wait::waitpid(
                nix::unistd::Pid::from_raw(pid as i32),
                Some(nix::sys::wait::WaitPidFlag::WNOHANG),
            );
            if let Ok(nix::sys::wait::WaitStatus::StillAlive) = wpid {
                // if it is, kill it
                nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGKILL,
                )
                .ok();
            }
        }

        // now let's check if any test banks are too old
        let mut banks = TEST_BANKS.lock().await;
        let now = Instant::now();
        let to_remove = banks
            .iter()
            .filter_map(|(name, bank)| {
                if now.duration_since(bank.last_accessed) > *MAX_TEST_BANK_OLDNESS {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for name in to_remove {
            banks.remove(&name);
        }
    }
}

async fn get_test_from_banks(repo: String, hash: String) -> Option<String> {
    let mut banks = TEST_BANKS.lock().await;
    let bank = banks.entry(repo.clone()).or_insert_with(|| {
        TestBank::from_hf(repo.clone()).unwrap_or_else(|e| {
            eprintln!("Failed to get test bank: {}", e);
            TestBank {
                repo: repo.clone(),
                map: HashMap::new(),
                last_accessed: Instant::now(),
            }
        })
    });
    bank.get_test(&hash).map(|s| s.to_string())
}

#[derive(Debug, Clone)]
pub struct TestBank {
    pub repo: String,
    pub map: HashMap<String, String>,
    pub last_accessed: Instant,
}

impl TestBank {
    pub fn from_hf(repo: String) -> Result<Self, Box<dyn std::error::Error>> {
        let api = hf_hub::api::sync::Api::new()?;
        let ds = candle_datasets::hub::from_hub(&api, repo.clone())?;
        let ds = ds.first().ok_or("No train split")?;
        let rowiter = ds.get_row_iter(None)?;
        let mut map = HashMap::new();
        for r in rowiter.into_iter() {
            let r = r?;
            let test = r.get_string(0)?.to_owned();
            let hash = r.get_string(1)?.to_owned();
            // make sure it's md5
            assert!(
                hash.len() == 32,
                "hash is not 32 chars long, got {} -- needs to be md5",
                hash.len()
            );
            map.insert(hash, test);
        }
        let last_accessed = Instant::now();
        Ok(Self {
            repo,
            map,
            last_accessed,
        })
    }

    pub fn get_test(&mut self, hash: &str) -> Option<String> {
        self.last_accessed = Instant::now();
        self.map.get(hash).cloned()
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    debug!(
        "memory limit: {} bytes (GB: {})",
        *MEMORY_LIMIT,
        *MEMORY_LIMIT / 1024 / 1024
    );
    let app = Router::new()
        .route("/py_exec", post(py_exec))
        .route("/any_exec", post(any_exec))
        .route("/py_coverage", post(coverage))
        .route("/health", get(health_check))
        .layer(DefaultBodyLimit::max(std::usize::MAX));

    let args: Vec<String> = std::env::args().collect();
    let port = args
        .iter()
        .position(|arg| arg == "--port")
        .and_then(|index| args.get(index + 1))
        .map(|port| port.to_string())
        .unwrap_or_else(|| "8000".to_string());

    let ip = args
        .iter()
        .position(|arg| arg == "--ip")
        .and_then(|index| args.get(index + 1))
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "0.0.0.0".to_string());

    let addr = format!("{}:{}", ip, port);

    let gc = tokio::spawn(garbage_collector());
    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
    gc.await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}

async fn create_temp_file(ext: &str) -> String {
    let idx = FILE_IDX.fetch_add(1, Ordering::SeqCst);
    // temp dir
    let temp_dir = std::path::PathBuf::from("/dev/shm/codeexec"); // uses /dev/shm for miles faster IO
    if !temp_dir.exists() {
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    }
    let filename = format!("{}/{}.{}", temp_dir.to_string_lossy(), idx, ext);
    filename
}

// error for Result<Output, ExecError>
#[derive(Debug)]
enum ExecError {
    IoError(std::io::Error),
    Utf8Error(std::string::FromUtf8Error),
    Timeout,
}

impl From<std::io::Error> for ExecError {
    fn from(e: std::io::Error) -> Self {
        ExecError::IoError(e)
    }
}

impl From<std::string::FromUtf8Error> for ExecError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        ExecError::Utf8Error(e)
    }
}

type ExecResult = Result<Output, ExecError>;

async fn run_program_with_timeout(
    program: &str,
    args: &[&str],
    stdin_data: &[u8],
    timeout: Duration,
) -> ExecResult {
    let mut child = unsafe {
        tokio::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped())
            // NOTE: this is the unsafe bit
            .pre_exec(move || {
                // restrict gid and uid
                nix::unistd::setgid(nix::unistd::Gid::from_raw(1000))?;
                nix::unistd::setuid(nix::unistd::Uid::from_raw(1000))?;
                Ok(())
            })
            .spawn()?
    };
    if !stdin_data.is_empty() {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(stdin_data).await?;
    }
    let pid = child.id();
    if let Some(pid) = pid {
        PID_POOL.lock().await.push((pid, Instant::now(), timeout));
    }
    let output = tokio::time::timeout(timeout, child.wait()).await;
    let mut stdout = child
        .stdout
        .take()
        .ok_or(ExecError::IoError(std::io::Error::from_raw_os_error(0)))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or(ExecError::IoError(std::io::Error::from_raw_os_error(0)))?;
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    match output {
        Ok(output) => match output {
            Ok(output) => {
                stdout.read_to_end(&mut stdout_buf).await?;
                stderr.read_to_end(&mut stderr_buf).await?;
                Ok(std::process::Output {
                    status: output,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                })
            }
            Err(e) => {
                child.kill().await.ok();
                Err(ExecError::IoError(e))
            }
        },
        Err(_) => {
            child.kill().await.ok();
            Err(ExecError::Timeout)
        }
    }
}

fn out_to_res_helper(output: ExecResult) -> (i8, String) {
    match output {
        Ok(o) if o.status.code().unwrap_or(-1) == 0 => {
            (0, String::from_utf8_lossy(&o.stdout).to_string())
        }
        Ok(o) => (1, String::from_utf8_lossy(&o.stderr).to_string()),
        Err(ExecError::Timeout) => (1, "Timeout".to_string()),
        Err(ExecError::IoError(e)) => (1, e.to_string()),
        Err(ExecError::Utf8Error(e)) => (1, e.to_string()),
    }
}

fn out_to_res(output: ExecResult) -> String {
    let (status, output) = out_to_res_helper(output);
    format!("{}\n{}", status, output)
}

fn out_to_res_json(output: ExecResult) -> String {
    let (status, output) = out_to_res_helper(output);
    // make it {"status": <status>, "output": <output>}
    serde_json::to_string(&serde_json::json!({ "status": status, "output": output }))
        .unwrap_or_else(|_| "-1\nFailed to serialize output".to_string())
}

async fn run_py_code(input: JsonInput) -> (String, String) {
    let tempfile = create_temp_file("py").await;
    let mut code = input.code;
    if let Some((testbank, hash)) = input.testhash {
        let test = get_test_from_banks(testbank, hash).await;
        if let Some(test) = test {
            code.push_str("\n\n");
            code.push_str(&test);
        }
    }
    tokio::fs::write(&tempfile, code).await.unwrap();
    let output = run_program_with_timeout(
        "bash",
        &[
            "-c",
            &format!("ulimit -v {}; python3 {}", *MEMORY_LIMIT, tempfile),
        ],
        input.stdin.unwrap_or_default().as_bytes(),
        Duration::from_secs(input.timeout),
    )
    .await;

    let res = if input.json_resp.unwrap_or(false) {
        out_to_res_json(output)
    } else {
        out_to_res(output)
    };

    debug!("{}: {}", tempfile, res);
    (res, tempfile)
}

async fn run_multipl_e_prog(code: &str, lang: &str, timeout: u64) -> (String, String) {
    let tempfile = create_temp_file(lang).await;
    tokio::fs::write(&tempfile, code).await.unwrap();

    // method:
    // cwd into $CRATE_DIR/MultiPL-E/evaluation/src
    // run `python3 -c "import eval_$lang; eval_$lang.eval_script('$tempfile')"`
    let output = run_program_with_timeout(
        "python3",
        &[
            "-c",
            &format!(
                "import sys; sys.path.append('{}/MultiPL-E/evaluation/src'); import json; import eval_{}; print(json.dumps(eval_{}.eval_script('{}')))",
                *CRATE_DIR, lang, lang, tempfile
            ),
        ],
        &[], // TODO: add stdin opt for multipl-e
        Duration::from_secs(timeout),
    ).await;
    let res = out_to_res(output);

    debug!("{}: {}", tempfile, res);
    (res, tempfile)
}

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct JsonInput {
    /// Code to run
    code: String,
    /// Timeout in seconds
    timeout: u64,
    /// Optional stdin. By default, no stdin is provided
    stdin: Option<String>,
    /// Optional language. By default, it's python
    lang: Option<String>,
    /// Enable json responses instead of the "<status>\n<output>" format.
    /// By default the \n format is used -- faster to parse
    json_resp: Option<bool>,
    /// Optional testbank name that contains the tests and the hash of the test to run. Tests are appended to the code
    testhash: Option<(String, String)>,
}

#[derive(Serialize)]
struct CoverageOutput {
    coverage: i8,
}

fn get_json_input(json: &str) -> Result<JsonInput, serde_json::Error> {
    serde_json::from_str(json)
}

async fn coverage(json: String) -> String {
    let input = match get_json_input(&json) {
        Ok(input) => input,
        Err(_) => return "-1".to_string(),
    };

    let tempfile = create_temp_file("py").await;
    tokio::fs::write(&tempfile, &input.code).await.unwrap();
    let cov_file = format!("{}.cov", tempfile);
    let thunk = async {
        let output = run_program_with_timeout(
            "coverage",
            &["run", "--data-file", cov_file.as_str(), tempfile.as_str()],
            &[], // no stdin
            Duration::from_secs(input.timeout),
        )
        .await
        .ok()?;
        if output.status.code()? != 0 {
            return None;
        }
        let output = run_program_with_timeout(
            "coverage",
            &["report", "--data-file", cov_file.as_str()],
            &[], // no stdin
            Duration::from_secs(10),
        )
        .await
        .ok()?;
        if output.status.code()? != 0 {
            return None;
        }
        let stdout = String::from_utf8(output.stdout).ok()?;
        let mut cov_percentage: i8 = 0;
        let mut next_is_cov = false;
        for line in stdout.lines() {
            if next_is_cov {
                let spacesplit = line
                    .split(' ')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim_end_matches('%'))
                    .collect::<Vec<_>>();
                cov_percentage = spacesplit.get(3)?.parse().ok()?;
                break;
            } else if line.starts_with("---------") {
                next_is_cov = true;
            }
        }

        Some(cov_percentage)
    };
    let res = thunk.await.unwrap_or(-1);
    tokio::fs::remove_file(&tempfile).await.unwrap();
    tokio::fs::remove_file(&cov_file).await.ok(); // the file may not exist

    serde_json::to_string(&CoverageOutput { coverage: res }).unwrap_or_else(|_| "-1".to_string())
}

async fn py_exec(json: String) -> String {
    let input = match get_json_input(&json) {
        Ok(input) => input,
        Err(_) => return "1\nInvalid JSON input".to_string(),
    };

    let (res, tempfile) = run_py_code(input).await;
    tokio::fs::remove_file(&tempfile).await.unwrap();
    res
}

async fn any_exec(json: String) -> String {
    let input = match get_json_input(&json) {
        Ok(input) => input,
        Err(_) => return "1\nInvalid JSON input".to_string(),
    };

    if input.testhash.is_some() {
        return "-1\nTesthash is not supported for this endpoint".to_string();
    }
    let lang = input.lang.unwrap_or_else(|| "py".to_string());
    let (res, tempfile) = run_multipl_e_prog(&input.code, &lang, input.timeout).await;
    tokio::fs::remove_file(&tempfile).await.unwrap();
    res
}
