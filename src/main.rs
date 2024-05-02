use axum::{routing::post, Router};
use lazy_static::lazy_static;
use std::{
    process::Output,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/py_exec", post(py_exec))
        .route("/any_exec", post(py_exec))
        .route("/py_coverage", post(coverage));

    axum::Server::bind(&"0.0.0.0:8000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

lazy_static! {
    static ref FILE_IDX: AtomicUsize = AtomicUsize::new(0);
    static ref CPU_SEMAPHORE: tokio::sync::Semaphore =
        tokio::sync::Semaphore::new(std::thread::available_parallelism().unwrap().into());
}

async fn create_temp_file() -> String {
    let idx = FILE_IDX.fetch_add(1, Ordering::SeqCst);
    // temp dir
    let temp_dir = std::env::temp_dir().join("codeexec");
    if !temp_dir.exists() {
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    }
    let filename = format!("{}/{}.py", temp_dir.to_string_lossy(), idx);
    filename
}

async fn run_program_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Option<Output> {
    let _permit = CPU_SEMAPHORE.acquire().await.unwrap();
    let child = tokio::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let child_id = child.id().unwrap();
    let output = tokio::time::timeout(timeout, child.wait_with_output()).await;
    match output {
        Ok(output) => match output {
            Ok(output) => Some(output),
            Err(_) => {
                let _ = tokio::process::Command::new("kill")
                    .arg("-9")
                    .arg(format!("{}", child_id))
                    .spawn();
                None
            }
        },
        Err(_) => {
            let _ = tokio::process::Command::new("kill")
                .arg("-9")
                .arg(format!("{}", child_id))
                .spawn();
            None
        }
    }
}

async fn run_code(code: &str) -> (String, String) {
    let tempfile = create_temp_file().await;
    tokio::fs::write(&tempfile, code).await.unwrap();
    // check for timeout
    let output =
        run_program_with_timeout("python3", &[tempfile.as_str()], Duration::from_secs(25)).await;

    let res = match output.as_ref().map(|o| o.status.code().unwrap_or(-1)) {
        Some(0) => format!("0\n{}", String::from_utf8_lossy(&output.unwrap().stdout)),
        Some(-1) => "1\nTimeout".to_string(),
        _ => format!(
            "1\n{}",
            output
                .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                .unwrap_or_default(),
        ),
    };

    println!("{}: {}", tempfile, res);
    (res, tempfile)
}

/// hacky but i'm lazy
fn get_string_json(json: String, key: &str) -> String {
    serde_json::from_str::<serde_json::Value>(&json)
        .map(|v| {
            v.get(key)
                .unwrap_or(&serde_json::Value::Null)
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default()
}

async fn coverage(json: String) -> String {
    let code = get_string_json(json, "code");
    let tempfile = create_temp_file().await;
    tokio::fs::write(&tempfile, code).await.unwrap();
    let cov_file = format!("{}.cov", tempfile);
    let thunk = async {
        let output = run_program_with_timeout(
            "coverage",
            &["run", "--data-file", cov_file.as_str(), tempfile.as_str()],
            Duration::from_secs(60),
        )
        .await?;
        if output.status.code()? != 0 {
            return None;
        }
        let output = run_program_with_timeout(
            "coverage",
            &["report", "--data-file", cov_file.as_str()],
            Duration::from_secs(10),
        )
        .await?;
        if output.status.code()? != 0 {
            return None;
        }
        let stdout = String::from_utf8(output.stdout).ok()?;
        let mut cov_percentage: u8 = 0;
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

        Some(cov_percentage.to_string())
    };
    let res = thunk.await.unwrap_or("-1".to_string());
    tokio::fs::remove_file(&tempfile).await.unwrap();
    tokio::fs::remove_file(&cov_file).await.ok(); // the file may not exist
    res
}

async fn py_exec(json: String) -> String {
    let code = get_string_json(json, "code");
    let (res, tempfile) = run_code(&code).await;
    tokio::fs::remove_file(&tempfile).await.unwrap();
    res
}
