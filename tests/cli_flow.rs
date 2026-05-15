use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

struct TestApp {
    _dir: TempDir,
    db: String,
}

impl TestApp {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("workoutd.sqlite3").display().to_string();
        Self { _dir: dir, db }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("workoutd").unwrap();
        cmd.env("WORKOUTD_DB", &self.db);
        cmd
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self
            .cmd()
            .args(["--json"])
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        serde_json::from_slice(&output).unwrap()
    }
}

#[test]
fn full_cli_flow_tracks_history() {
    let app = TestApp::new();

    let gym = app.json(&["gym", "add", "--name", "Main Gym"]);
    let gym_id = gym["id"].as_i64().unwrap();

    let exercise = app.json(&[
        "exercise",
        "add",
        "--name",
        "Bench Press",
        "--kind",
        "freeweight",
        "--tag",
        "chest",
    ]);
    let exercise_id = exercise["id"].as_i64().unwrap();

    app.cmd()
        .args(["session", "start", "--gym", &gym_id.to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("started session"));

    let first_entry = app.json(&["log", "exercise", &exercise_id.to_string()]);
    let first_entry_id = first_entry["entry"]["id"].as_i64().unwrap();
    assert_eq!(first_entry["history"].as_array().unwrap().len(), 0);

    app.cmd()
        .args([
            "log",
            "set",
            "--exercise-entry",
            &first_entry_id.to_string(),
            "--reps",
            "5",
            "--weight",
            "100",
            "--unit",
            "kg",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("100kg x 5"));

    app.cmd().args(["session", "finish"]).assert().success();
    app.cmd()
        .args(["session", "start", "--gym", &gym_id.to_string()])
        .assert()
        .success();

    let second_entry = app.json(&["log", "exercise", &exercise_id.to_string()]);
    assert_eq!(second_entry["history"].as_array().unwrap().len(), 1);
    assert_eq!(
        second_entry["history"][0]["sets"][0]["reps"]
            .as_i64()
            .unwrap(),
        5
    );
}

#[test]
fn session_delete_requires_confirmation_and_removes_finished_session() {
    let app = TestApp::new();

    let gym = app.json(&["gym", "add", "--name", "Main Gym"]);
    let gym_id = gym["id"].as_i64().unwrap();
    let exercise = app.json(&[
        "exercise",
        "add",
        "--name",
        "Squat",
        "--kind",
        "freeweight",
        "--tag",
        "quad",
    ]);
    let exercise_id = exercise["id"].as_i64().unwrap();
    let session = app.json(&["session", "start", "--gym", &gym_id.to_string()]);
    let session_id = session["id"].as_str().unwrap().to_string();
    assert_eq!(session_id.len(), 36);
    let session_date = &session["started_at"].as_str().unwrap()[..10];
    let entry = app.json(&["log", "exercise", &exercise_id.to_string()]);
    let entry_id = entry["entry"]["id"].as_i64().unwrap();
    app.cmd()
        .args([
            "log",
            "set",
            "--exercise-entry",
            &entry_id.to_string(),
            "--reps",
            "5",
            "--weight",
            "100",
            "--unit",
            "kg",
        ])
        .assert()
        .success();
    app.cmd().args(["session", "finish"]).assert().success();

    app.cmd()
        .args(["session", "delete", &session_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--yes"));

    app.cmd()
        .args(["session", "list", "--date", session_date])
        .assert()
        .success()
        .stdout(predicate::str::contains(&session_id));

    app.cmd()
        .args(["session", "delete", &session_id, "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("deleted session"));

    app.cmd()
        .args(["session", "delete", &session_id, "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn log_exercise_suggests_similar_names_without_resolving() {
    let app = TestApp::new();

    let gym = app.json(&["gym", "add", "--name", "Main Gym"]);
    let gym_id = gym["id"].as_i64().unwrap();
    app.json(&[
        "exercise",
        "add",
        "--name",
        "Bench Press",
        "--kind",
        "freeweight",
        "--tag",
        "chest",
    ]);
    app.cmd()
        .args(["session", "start", "--gym", &gym_id.to_string()])
        .assert()
        .success();

    app.cmd()
        .args(["log", "exercise", "bench"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("similar exercises"))
        .stderr(predicate::str::contains("Bench Press"));
}

#[test]
fn machine_exercise_rejects_machine_from_other_gym() {
    let app = TestApp::new();

    let gym_a = app.json(&["gym", "add", "--name", "A"]);
    let gym_b = app.json(&["gym", "add", "--name", "B"]);
    let gym_a_id = gym_a["id"].as_i64().unwrap();
    let gym_b_id = gym_b["id"].as_i64().unwrap();

    let machine = app.json(&[
        "machine",
        "add",
        "--gym",
        &gym_b_id.to_string(),
        "--name",
        "Leg Press",
        "--type",
        "leg-press",
    ]);
    let machine_id = machine["id"].as_i64().unwrap();

    let exercise = app.json(&[
        "exercise",
        "add",
        "--name",
        "Leg Press",
        "--kind",
        "machine",
    ]);
    let exercise_id = exercise["id"].as_i64().unwrap();

    app.cmd()
        .args(["session", "start", "--gym", &gym_a_id.to_string()])
        .assert()
        .success();

    app.cmd()
        .args([
            "log",
            "exercise",
            &exercise_id.to_string(),
            "--machine",
            &machine_id.to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found in active gym"));
}

#[test]
fn machine_logging_suggests_only_active_gym_machines_and_shows_notes() {
    let app = TestApp::new();

    let gym_a = app.json(&["gym", "add", "--name", "A"]);
    let gym_b = app.json(&["gym", "add", "--name", "B"]);
    let gym_a_id = gym_a["id"].as_i64().unwrap();
    let gym_b_id = gym_b["id"].as_i64().unwrap();

    let active_machine = app.json(&[
        "machine",
        "add",
        "--gym",
        &gym_a_id.to_string(),
        "--name",
        "Leg Press A",
        "--type",
        "leg-press",
    ]);
    let active_machine_id = active_machine["id"].as_i64().unwrap();
    app.json(&[
        "machine",
        "add",
        "--gym",
        &gym_b_id.to_string(),
        "--name",
        "Leg Press B",
        "--type",
        "leg-press",
    ]);
    app.cmd()
        .args([
            "machine",
            "note",
            "add",
            &active_machine_id.to_string(),
            "--note",
            "seat 4",
        ])
        .assert()
        .success();

    let exercise = app.json(&[
        "exercise",
        "add",
        "--name",
        "Leg Press",
        "--kind",
        "machine",
        "--tag",
        "quad",
    ]);
    let exercise_id = exercise["id"].as_i64().unwrap();

    app.cmd()
        .args(["session", "start", "--gym", &gym_a_id.to_string()])
        .assert()
        .success();

    app.cmd()
        .args([
            "log",
            "exercise",
            &exercise_id.to_string(),
            "--machine",
            "press",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Leg Press A"))
        .stderr(predicate::str::contains("Leg Press B").not());

    app.cmd()
        .args([
            "log",
            "exercise",
            &exercise_id.to_string(),
            "--machine",
            "Leg Press A",
            "--machine-note",
            "pin 90",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("machine notes"))
        .stdout(predicate::str::contains("seat 4"));

    let notes = app.json(&["machine", "note", "list", &active_machine_id.to_string()]);
    assert_eq!(notes.as_array().unwrap().len(), 2);
}

#[test]
fn daemon_once_smoke_test() {
    let app = TestApp::new();
    let mut cmd = Command::cargo_bin("workoutd-daemon").unwrap();
    cmd.env("WORKOUTD_DB", &app.db)
        .args(["--once"])
        .assert()
        .success()
        .stderr(predicate::str::contains("healthy"));
}

#[test]
fn tag_allowlist_is_discoverable_and_enforced() {
    let app = TestApp::new();

    app.cmd()
        .args(["tag", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("side delt"))
        .stdout(predicate::str::contains("abs"));

    app.cmd()
        .args([
            "exercise",
            "add",
            "--name",
            "Curl",
            "--kind",
            "freeweight",
            "--tag",
            "arms",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown tag"));
}

#[test]
fn exercise_add_requires_force_when_similar_exercise_exists() {
    let app = TestApp::new();

    app.json(&[
        "exercise",
        "add",
        "--name",
        "Bench Press",
        "--kind",
        "freeweight",
        "--tag",
        "chest",
    ]);

    app.cmd()
        .args([
            "exercise",
            "add",
            "--name",
            "bench",
            "--kind",
            "freeweight",
            "--tag",
            "chest",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("similar exercises"));

    app.cmd()
        .args([
            "exercise",
            "add",
            "--name",
            "bench",
            "--kind",
            "freeweight",
            "--tag",
            "chest",
            "--force",
        ])
        .assert()
        .success();
}
