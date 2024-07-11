use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use lazy_static::lazy_static;
use std::{
    process::Output,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

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

    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
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

async fn run_py_code(code: &str, timeout: u64, stdin: String, json_resp: bool) -> (String, String) {
    let tempfile = create_temp_file("py").await;
    let orphan_timeout = timeout + 5; // give it 5 seconds to clean up
    tokio::fs::write(&tempfile, code).await.unwrap();
    let output = run_program_with_timeout(
        "bash",
        &[
            "-c",
            &format!(
                "ulimit -v {}; timeout -k 5 {} python3 {}",
                *MEMORY_LIMIT, orphan_timeout, tempfile
            ),
        ],
        stdin.as_bytes(),
        Duration::from_secs(timeout),
    )
    .await;

    let res = if json_resp {
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
    code: String,
    timeout: u64,
    stdin: Option<String>,
    lang: Option<String>,
    json_resp: Option<bool>,
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

    let (res, tempfile) = run_py_code(
        &input.code,
        input.timeout,
        input.stdin.unwrap_or_default(),
        input.json_resp.unwrap_or(false),
    )
    .await;
    tokio::fs::remove_file(&tempfile).await.unwrap();
    res
}

async fn any_exec(json: String) -> String {
    let input = match get_json_input(&json) {
        Ok(input) => input,
        Err(_) => return "1\nInvalid JSON input".to_string(),
    };

    let lang = input.lang.unwrap_or_default();
    let (res, tempfile) = run_multipl_e_prog(&input.code, &lang, input.timeout).await;
    tokio::fs::remove_file(&tempfile).await.unwrap();
    res
}
