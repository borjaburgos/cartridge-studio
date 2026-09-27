use cartridge_app::worker::{Event, Job};
use cartridge_app::Error;
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn job(script: &str) -> Job {
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    // A syntactically complete request is read before every fixture replies.
    Job::with_command(command, &request()).unwrap()
}
fn request() -> cartridge_core::service::Request {
    cartridge_core::service::Request::new("profiles")
}
fn finish(job: &Job) -> std::result::Result<serde_json::Value, Error> {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if let Some(Event::Finished(r)) = job.poll() {
            return r;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("Worker did not finish within fixture deadline")
}
#[test]
fn result_is_not_success_until_the_process_exits_cleanly() {
    let success = job("read request; printf '%s\n' '{\"event\":\"result\",\"result\":{\"ok\":true}}'; sleep 0.1; exit 0");
    assert!(success.poll().is_none());
    assert_eq!(finish(&success).unwrap()["ok"], true);
    let failed = job("read request; printf '%s\n' '{\"event\":\"result\",\"result\":{}}'; exit 7");
    assert_eq!(finish(&failed).unwrap_err().code, "WORKER_EXITED");
}
#[test]
fn malformed_and_truncated_protocol_never_look_complete() {
    for script in ["read request; printf 'not json\n'", "read request; exit 0", "read request; printf '%s\n' '{\"event\":\"result\",\"result\":{}}' '{\"event\":\"result\",\"result\":{}}'"] {
        assert_eq!(finish(&job(script)).unwrap_err().code, "WORKER_PROTOCOL");
    }
}
#[test]
fn stderr_is_drained_and_errors_keep_the_recovery_action() {
    let worker = job("read request; i=0; while [ $i -lt 8000 ]; do echo 'diagnostic message diagnostic message' >&2; i=$((i + 1)); done; printf '%s\n' '{\"event\":\"error\",\"error\":\"RESTORE_REQUIRED\",\"message\":\"Interrupted\",\"action\":\"Restore source.gb\",\"details\":{\"bank\":4}}'; exit 2");
    let error = finish(&worker).unwrap_err();
    assert_eq!(error.action, "Restore source.gb");
    assert_eq!(error.details["bank"], 4);
}
#[test]
fn cancellation_is_cooperative_and_reports_completion_after_cleanup() {
    let mut worker = job("read request; trap 'printf \"%s\\n\" \"{\\\"event\\\":\\\"error\\\",\\\"error\\\":\\\"INTERRUPTED\\\",\\\"message\\\":\\\"Stopped\\\",\\\"action\\\":\\\"Keep backup\\\"}\"; exit 130' TERM; printf '%s\n' '{\"event\":\"progress\",\"message\":\"ready\"}'; while :; do sleep 0.05; done");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !matches!(worker.poll(), Some(Event::Progress(_))) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    worker.stop();
    assert!(worker.stopping);
    assert_eq!(finish(&worker).unwrap_err().code, "INTERRUPTED");
}
