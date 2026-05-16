use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
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
fn session_export_outputs_full_session_json() {
    let app = TestApp::new();

    let gym = app.json(&["gym", "add", "--name", "Main Gym"]);
    let gym_id = gym["id"].as_i64().unwrap();
    let machine = app.json(&[
        "machine",
        "add",
        "--gym",
        &gym_id.to_string(),
        "--name",
        "Leg Press",
        "--type",
        "leg-press",
        "--brand",
        "Hammer Strength",
    ]);
    let machine_uuid = machine["uuid"].as_str().unwrap().to_string();
    app.cmd()
        .args(["machine", "note", "add", &machine_uuid, "--note", "pin 90"])
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
    let session = app.json(&["session", "start", "--gym", &gym_id.to_string()]);
    let session_id = session["id"].as_str().unwrap().to_string();
    let entry = app.json(&[
        "log",
        "exercise",
        &exercise_id.to_string(),
        "--machine",
        &machine_uuid,
    ]);
    let entry_id = entry["entry"]["id"].as_i64().unwrap();
    app.cmd()
        .args([
            "log",
            "set",
            "--exercise-entry",
            &entry_id.to_string(),
            "--reps",
            "10",
            "--weight",
            "180",
            "--unit",
            "kg",
        ])
        .assert()
        .success();
    app.cmd().args(["session", "finish"]).assert().success();

    let output = app
        .cmd()
        .args(["session", "export", &session_id])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let exported: Value = serde_json::from_slice(&output).unwrap();

    assert_eq!(exported["session"]["id"], session_id);
    assert_eq!(exported["exercises"].as_array().unwrap().len(), 1);
    assert_eq!(
        exported["exercises"][0]["entry"]["exercise_name"],
        "Leg Press"
    );
    assert_eq!(
        exported["exercises"][0]["machine"]["brand"],
        "Hammer Strength"
    );
    assert_eq!(exported["exercises"][0]["sets"][0]["reps"], 10);
    assert_eq!(
        exported["exercises"][0]["machine_notes"][0]["note"],
        "pin 90"
    );
}

#[test]
fn session_import_restores_exported_session_json() {
    let source = TestApp::new();
    let gym = source.json(&["gym", "add", "--name", "Main Gym"]);
    let gym_id = gym["id"].as_i64().unwrap();
    let machine = source.json(&[
        "machine",
        "add",
        "--gym",
        &gym_id.to_string(),
        "--name",
        "Leg Press",
        "--type",
        "leg-press",
        "--brand",
        "Hammer Strength",
    ]);
    let machine_uuid = machine["uuid"].as_str().unwrap().to_string();
    source
        .cmd()
        .args(["machine", "note", "add", &machine_uuid, "--note", "pin 90"])
        .assert()
        .success();
    let exercise = source.json(&[
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
    let session = source.json(&["session", "start", "--gym", &gym_id.to_string()]);
    let session_id = session["id"].as_str().unwrap().to_string();
    let entry = source.json(&[
        "log",
        "exercise",
        &exercise_id.to_string(),
        "--machine",
        &machine_uuid,
    ]);
    let entry_id = entry["entry"]["id"].as_i64().unwrap();
    source
        .cmd()
        .args([
            "log",
            "set",
            "--exercise-entry",
            &entry_id.to_string(),
            "--reps",
            "10",
            "--weight",
            "180",
            "--unit",
            "kg",
        ])
        .assert()
        .success();
    source.cmd().args(["session", "finish"]).assert().success();

    let exported = source.json(&["session", "export", &session_id]);
    let export_dir = tempfile::tempdir().unwrap();
    let export_path = export_dir.path().join("session.json");
    fs::write(
        &export_path,
        serde_json::to_string_pretty(&exported).unwrap(),
    )
    .unwrap();

    let target = TestApp::new();
    let imported = target.json(&["session", "import", export_path.to_str().unwrap()]);

    assert_eq!(imported["session"]["id"], session_id);
    assert_eq!(
        imported["exercises"][0]["entry"]["exercise_name"],
        "Leg Press"
    );
    assert_eq!(imported["exercises"][0]["machine"]["uuid"], machine_uuid);
    assert_eq!(
        imported["exercises"][0]["machine"]["brand"],
        "Hammer Strength"
    );
    assert_eq!(imported["exercises"][0]["sets"][0]["reps"], 10);
    assert_eq!(
        imported["exercises"][0]["machine_notes"][0]["note"],
        "pin 90"
    );

    target
        .cmd()
        .args(["session", "import", export_path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn html_export_renders_session_json() {
    let dir = tempfile::tempdir().unwrap();
    let input_path = dir.path().join("session.json");
    fs::write(
        &input_path,
        r#"{
  "session": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "gym_id": 1,
    "gym_name": "Main Gym",
    "started_at": "2026-05-16 10:00:00",
    "finished_at": "2026-05-16 11:00:00",
    "notes": "Push <heavy>"
  },
  "exercises": [
    {
      "entry": {
        "id": 1,
        "session_id": "550e8400-e29b-41d4-a716-446655440000",
        "exercise_id": 1,
        "exercise_name": "Bench & Press",
        "kind": "freeweight",
        "machine_id": null,
        "machine_name": null,
        "position": 1,
        "notes": null,
        "created_at": "2026-05-16 10:05:00"
      },
      "machine": null,
      "sets": [
        {
          "id": 1,
          "session_exercise_id": 1,
          "position": 1,
          "weight_value": 100,
          "weight_unit": "kg",
          "reps": 5,
          "created_at": "2026-05-16 10:06:00"
        }
      ],
      "machine_notes": []
    }
  ]
}"#,
    )
    .unwrap();

    let output = Command::cargo_bin("workoutd-html")
        .unwrap()
        .args([
            "--input",
            input_path.to_str().unwrap(),
            "--timezone",
            "Asia/Seoul",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let html = String::from_utf8(output).unwrap();

    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("Bench &amp; Press"));
    assert!(html.contains("Push &lt;heavy&gt;"));
    assert!(html.contains("Asia/Seoul"));
    assert!(html.contains("100 kg"));
}

#[test]
fn html_export_renders_exercise_history_chart() {
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
        .success();
    let first_entry = app.json(&["log", "exercise", &exercise_id.to_string()]);
    let first_entry_id = first_entry["entry"]["id"].as_i64().unwrap();
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
        .success();
    app.cmd().args(["session", "finish"]).assert().success();

    app.cmd()
        .args(["session", "start", "--gym", &gym_id.to_string()])
        .assert()
        .success();
    let second_entry = app.json(&["log", "exercise", &exercise_id.to_string()]);
    let second_entry_id = second_entry["entry"]["id"].as_i64().unwrap();
    app.cmd()
        .args([
            "log",
            "set",
            "--exercise-entry",
            &second_entry_id.to_string(),
            "--reps",
            "8",
            "--weight",
            "105",
            "--unit",
            "kg",
        ])
        .assert()
        .success();
    app.cmd().args(["session", "finish"]).assert().success();

    let output = Command::cargo_bin("workoutd-html")
        .unwrap()
        .env("WORKOUTD_DB", &app.db)
        .args([
            "--exercise-history",
            "Bench Press",
            "--timezone",
            "Asia/Seoul",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let html = String::from_utf8(output).unwrap();

    assert!(html.contains("Exercise history"));
    assert!(html.contains("Volume and max weight by session"));
    assert!(html.contains("Bench Press"));
    assert!(html.contains("history-chart"));
    assert!(html.contains("chart-legend"));
    assert!(html.contains("500 kg-reps"));
    assert!(html.contains("105 kg"));
}

#[test]
fn gym_names_resolve_exactly_and_suggest_fuzzy_matches() {
    let app = TestApp::new();

    app.json(&["gym", "add", "--name", "Main Gym"]);

    app.cmd()
        .args(["session", "start", "--gym", "Main Gym"])
        .assert()
        .success()
        .stdout(predicate::str::contains("started session"));
    app.cmd().args(["session", "cancel"]).assert().success();

    app.cmd()
        .args([
            "machine",
            "add",
            "--gym",
            "main-gym",
            "--name",
            "Cable Stack",
            "--type",
            "cable",
        ])
        .assert()
        .success();

    app.cmd()
        .args(["session", "start", "--gym", "main"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("similar gyms"))
        .stderr(predicate::str::contains("Main Gym"));
}

#[test]
fn machine_brand_presets_are_discoverable_and_canonicalized() {
    let app = TestApp::new();

    app.cmd()
        .args(["machine", "brand", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hammer Strength"))
        .stdout(predicate::str::contains("Newtech Wellness"))
        .stdout(predicate::str::contains("Technogym"));

    let gym = app.json(&["gym", "add", "--name", "Main Gym"]);
    let gym_id = gym["id"].as_i64().unwrap();
    let machine = app.json(&[
        "machine",
        "add",
        "--gym",
        &gym_id.to_string(),
        "--name",
        "Leg Press",
        "--type",
        "leg-press",
        "--brand",
        "hammer strength",
        "--load-kind",
        "plate-loaded",
    ]);

    assert_eq!(machine["brand"], "Hammer Strength");
    assert_eq!(machine["load_kind"], "plate-loaded");

    let newtech = app.json(&[
        "machine",
        "add",
        "--gym",
        &gym_id.to_string(),
        "--name",
        "Chest Press",
        "--type",
        "chest-press",
        "--brand",
        "newtech",
        "--load-kind",
        "pin-loaded",
    ]);
    assert_eq!(newtech["brand"], "Newtech Wellness");
    assert_eq!(newtech["load_kind"], "pin-loaded");
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
    let machine_uuid = machine["uuid"].as_str().unwrap().to_string();

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
            &machine_uuid,
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
        "--load-kind",
        "plate-loaded",
    ]);
    let active_machine_uuid = active_machine["uuid"].as_str().unwrap().to_string();
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
            &active_machine_uuid,
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

    let lookup = app.json(&["machine", "lookup", "--gym", &gym_a_id.to_string(), "press"]);
    assert_eq!(lookup.as_array().unwrap().len(), 1);
    assert_eq!(lookup[0]["uuid"], active_machine_uuid);
    assert_eq!(lookup[0]["name"], "Leg Press A");
    assert_eq!(lookup[0]["load_kind"], "plate-loaded");

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
        .stderr(predicate::str::contains("requires a machine UUID"));

    app.cmd()
        .args([
            "log",
            "exercise",
            &exercise_id.to_string(),
            "--machine",
            &active_machine_uuid,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("machine notes"))
        .stdout(predicate::str::contains("seat 4"));

    let notes = app.json(&["machine", "note", "list", &active_machine_uuid]);
    assert_eq!(notes.as_array().unwrap().len(), 1);
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
