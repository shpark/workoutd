# workoutd

Local workout tracker with a Rust CLI, SQLite database, and lightweight systemd user service.

## Build

```sh
cargo build
```

The database defaults to:

```text
$XDG_DATA_HOME/workoutd/workoutd.sqlite3
```

Override it with:

```sh
export WORKOUTD_DB=/path/to/workoutd.sqlite3
```

## CLI Examples

```sh
workoutd gym add --name "Main Gym"
workoutd machine brand list
workoutd machine add --gym "Main Gym" --name "Leg Press" --type leg-press --brand "Hammer Strength"
workoutd --json machine lookup --gym "Main Gym" "Leg Press"
workoutd machine note add MACHINE_UUID --note "seat 4, back 2"
workoutd tag list
workoutd exercise add --name "Bench Press" --kind freeweight --tag chest --tag triceps
workoutd exercise add --name "Leg Press" --kind machine --tag quad --tag glute

workoutd session start --gym "Main Gym"
workoutd log exercise "Bench Press"
workoutd log set --reps 5 --weight 100 --unit kg
workoutd log exercise "Leg Press" --machine MACHINE_UUID
workoutd session finish
workoutd session list --date 2026-05-15
workoutd session export 550e8400-e29b-41d4-a716-446655440000
workoutd session import session.json
workoutd session delete 550e8400-e29b-41d4-a716-446655440000 --yes
```

Use `--json` anywhere in the command for JSON output:

```sh
workoutd --json exercise list --tag chest
```

When `workoutd log exercise EXERCISE_ID` adds an exercise to the active session, it prints the previous completed session entry for the same exercise by default. Use `--history N` to change the number of entries.

`--gym` accepts a gym id, exact name, or exact slug for session and machine commands. Fuzzy matches are suggestions only: `main` will suggest `Main Gym` if that exists, but it will not resolve to it automatically.

`workoutd log exercise` accepts an exercise id, exact name, or exact slug. Fuzzy matches are suggestions only: `bench` will suggest `Bench Press` if that exists, but it will not resolve to it automatically.

`workoutd exercise add` also checks for similar existing exercises. Use `--force` when you intentionally want a distinct exercise with a similar name.

Machine exercises accept `--machine MACHINE_UUID`, scoped to the active session gym. Use lookup to find the UUID by name, type, or brand:

```sh
workoutd --json machine lookup --gym "Main Gym" "leg press"
```

The lookup output includes `uuid`, `name`, `machine_type`, `brand`, and `model`, which makes it suitable for an AI agent to select a machine and pass the UUID to `workoutd log exercise`.

Machine brands have discoverable presets:

```sh
workoutd machine brand list
```

When adding a machine, preset brand names are matched case-insensitively and stored with canonical capitalization. Custom brand names are also accepted.

Machine notes are durable notes associated with a machine:

```sh
workoutd machine note add MACHINE_UUID --note "seat 4, back 2"
workoutd machine note list MACHINE_UUID
workoutd machine note archive 1
workoutd machine note restore 1
```

When logging a machine exercise, prior machine notes are printed with the usual exercise history.

Session IDs are random UUIDs in CLI and JSON output. `workoutd session cancel` deletes the active unfinished session; `workoutd session list --date YYYY-MM-DD` finds sessions started on a date; `workoutd session export SESSION_UUID` prints a full JSON export with logged exercises, machine details including brand, sets, and machine notes; `workoutd session import PATH` restores a session JSON export and creates missing gym, exercise, and machine records as needed; `workoutd session delete SESSION_UUID --yes` deletes that session, including its logged exercises and sets.

## AI Agent Migration Guide

Use JSON output when selecting IDs or UUIDs:

```sh
workoutd --json machine lookup --gym "Main Gym" "leg press"
```

For machine exercises, do not pass machine names to `log exercise`. Read the `uuid` field from lookup output and pass that value:

```sh
workoutd log exercise "Leg Press" --machine MACHINE_UUID
```

Machine notes are now machine-owned records. Do not pass `--machine-note` to `log exercise`; add or inspect notes through the machine note commands:

```sh
workoutd machine note add MACHINE_UUID --note "seat 4, back 2"
workoutd --json machine note list MACHINE_UUID
```

Gym arguments for session and machine commands can be a gym id, exact gym name, or exact gym slug. If a gym lookup is fuzzy, treat the command failure as a suggestion response and retry with an exact value from the message.

Machine lookup results are scoped to the requested gym and include `uuid`, `name`, `machine_type`, `brand`, and `model`. Prefer `uuid` for subsequent commands and use the descriptive fields only to choose the correct machine.

To move a session between databases, export it to a file and import that file into the target database:

```sh
workoutd session export SESSION_UUID > session.json
WORKOUTD_DB=/path/to/target.sqlite3 workoutd session import session.json
```

The import preserves the session UUID and timestamps, reuses matching catalog records, and creates missing gym, exercise, and machine records from the export. Import fails if the session UUID already exists.

Exercise tags are restricted to this allowlist:

```text
biceps
triceps
quad
glute
hamstring
back
chest
side delt
rear delt
front delt
abs
```

## Service

The daemon performs periodic database health checks and logs status.

```sh
cargo install --path .
mkdir -p ~/.config/systemd/user
cp systemd/workoutd-daemon.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now workoutd-daemon.service
```

Smoke test without staying resident:

```sh
workoutd-daemon --once
```
