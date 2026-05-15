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
        "push",
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
        .stderr(predicate::str::contains("does not belong"));
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
