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
workoutd machine add --gym 1 --name "Leg Press" --type leg-press
workoutd exercise add --name "Bench Press" --kind freeweight --tag push --tag chest
workoutd exercise add --name "Leg Press" --kind machine --tag legs

workoutd session start --gym 1
workoutd log exercise 1
workoutd log set --reps 5 --weight 100 --unit kg
workoutd session finish
```

Use `--json` anywhere in the command for JSON output:

```sh
workoutd --json exercise list --tag push
```

When `workoutd log exercise EXERCISE_ID` adds an exercise to the active session, it prints the previous completed session entry for the same exercise by default. Use `--history N` to change the number of entries.

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
