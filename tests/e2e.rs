#[test]
fn web_queue_and_process_integration() {
    let result = std::process::Command::new("python3")
        .arg(format!("{}/tests/e2e.py", env!("CARGO_MANIFEST_DIR")))
        .arg(env!("CARGO_BIN_EXE_lanprint"))
        .output()
        .expect("Python 3 is needed for the HTTP integration test");
    assert!(
        result.status.success(),
        "integration test failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}

#[test]
fn uploaded_jobs_use_the_durable_queue_without_monitoring() {
    let result = std::process::Command::new("python3")
        .arg(format!("{}/tests/uploads.py", env!("CARGO_MANIFEST_DIR")))
        .arg(env!("CARGO_BIN_EXE_lanprint"))
        .output()
        .expect("Python 3 is needed for the upload integration test");
    assert!(
        result.status.success(),
        "upload integration failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}
