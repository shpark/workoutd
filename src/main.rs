use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::path::PathBuf;
use std::str::FromStr;
use workoutd::{
    add_exercise, add_gym, add_machine, archive_exercise, archive_gym, archive_machine,
    cancel_session, current_session, default_db_path, finish_session, latest_session_exercise_id,
    list_exercises, list_gyms, list_machines, log_exercise, log_set, open_database,
    restore_exercise, restore_gym, restore_machine, start_session, update_exercise, ExerciseKind,
    HistoryEntry, LogExerciseResult, SetEntry, WeightUnit,
};

#[derive(Parser)]
#[command(name = "workoutd")]
#[command(about = "Local workout tracker")]
struct Cli {
    #[arg(long, global = true, env = "WORKOUTD_DB")]
    db: Option<PathBuf>,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Gym {
        #[command(subcommand)]
        command: GymCommand,
    },
    Machine {
        #[command(subcommand)]
        command: MachineCommand,
    },
    Exercise {
        #[command(subcommand)]
        command: ExerciseCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    Log {
        #[command(subcommand)]
        command: LogCommand,
    },
}

#[derive(Subcommand)]
enum GymCommand {
    Add(GymAdd),
    List(ListArchived),
    Archive(IdArg),
    Restore(IdArg),
}

#[derive(Args)]
struct GymAdd {
    #[arg(long)]
    name: String,
    #[arg(long)]
    address: Option<String>,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Subcommand)]
enum MachineCommand {
    Add(MachineAdd),
    List(MachineList),
    Archive(IdArg),
    Restore(IdArg),
}

#[derive(Args)]
struct MachineAdd {
    #[arg(long)]
    gym: i64,
    #[arg(long)]
    name: String,
    #[arg(long = "type")]
    machine_type: String,
    #[arg(long)]
    brand: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long = "settings-notes")]
    settings_notes: Option<String>,
}

#[derive(Args)]
struct MachineList {
    #[arg(long)]
    gym: i64,
    #[arg(long)]
    include_archived: bool,
}

#[derive(Subcommand)]
enum ExerciseCommand {
    Add(ExerciseAdd),
    List(ExerciseList),
    Update(ExerciseUpdate),
    Archive(IdArg),
    Restore(IdArg),
}

#[derive(Args)]
struct ExerciseAdd {
    #[arg(long)]
    name: String,
    #[arg(long)]
    kind: String,
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long)]
    description: Option<String>,
}

#[derive(Args)]
struct ExerciseList {
    #[arg(long)]
    tag: Option<String>,
    #[arg(long)]
    kind: Option<String>,
    #[arg(long)]
    include_archived: bool,
}

#[derive(Args)]
struct ExerciseUpdate {
    id: i64,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    kind: Option<String>,
    #[arg(long = "tag")]
    tags: Vec<String>,
    #[arg(long)]
    clear_tags: bool,
    #[arg(long)]
    description: Option<String>,
}

#[derive(Subcommand)]
enum SessionCommand {
    Start(SessionStart),
    Current,
    Finish(SessionFinish),
    Cancel,
}

#[derive(Args)]
struct SessionStart {
    #[arg(long)]
    gym: i64,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Args)]
struct SessionFinish {
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Subcommand)]
enum LogCommand {
    Exercise(LogExercise),
    Set(LogSet),
}

#[derive(Args)]
struct LogExercise {
    exercise_id: i64,
    #[arg(long)]
    machine: Option<i64>,
    #[arg(long)]
    notes: Option<String>,
    #[arg(long, default_value_t = 1)]
    history: usize,
}

#[derive(Args)]
struct LogSet {
    #[arg(long = "exercise-entry")]
    exercise_entry: Option<i64>,
    #[arg(long)]
    reps: i64,
    #[arg(long)]
    weight: Option<f64>,
    #[arg(long)]
    unit: Option<String>,
}

#[derive(Args)]
struct ListArchived {
    #[arg(long)]
    include_archived: bool,
}

#[derive(Args)]
struct IdArg {
    id: i64,
}

#[derive(Serialize)]
struct Status<'a> {
    status: &'a str,
    id: Option<i64>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.clone().map(Ok).unwrap_or_else(default_db_path)?;
    let mut conn = open_database(&db_path)?;

    match cli.command {
        Command::Gym { command } => match command {
            GymCommand::Add(args) => {
                let gym = add_gym(
                    &conn,
                    &args.name,
                    args.address.as_deref(),
                    args.notes.as_deref(),
                )?;
                emit(cli.json, &gym, || {
                    println!("added gym {} ({})", gym.id, gym.name);
                    Ok(())
                })
            }
            GymCommand::List(args) => {
                emit(cli.json, &list_gyms(&conn, args.include_archived)?, || {
                    for gym in list_gyms(&conn, args.include_archived)? {
                        println!(
                            "{}: {}{}",
                            gym.id,
                            gym.name,
                            archived_suffix(&gym.archived_at)
                        );
                    }
                    Ok(())
                })
            }
            GymCommand::Archive(args) => {
                archive_gym(&conn, args.id)?;
                emit_status(cli.json, "archived", args.id)
            }
            GymCommand::Restore(args) => {
                restore_gym(&conn, args.id)?;
                emit_status(cli.json, "restored", args.id)
            }
        },
        Command::Machine { command } => match command {
            MachineCommand::Add(args) => {
                let machine = add_machine(
                    &conn,
                    args.gym,
                    &args.name,
                    &args.machine_type,
                    args.brand.as_deref(),
                    args.model.as_deref(),
                    args.settings_notes.as_deref(),
                )?;
                emit(cli.json, &machine, || {
                    println!("added machine {} ({})", machine.id, machine.name);
                    Ok(())
                })
            }
            MachineCommand::List(args) => emit(
                cli.json,
                &list_machines(&conn, args.gym, args.include_archived)?,
                || {
                    for machine in list_machines(&conn, args.gym, args.include_archived)? {
                        println!(
                            "{}: {} [{}]{}",
                            machine.id,
                            machine.name,
                            machine.machine_type,
                            archived_suffix(&machine.archived_at)
                        );
                    }
                    Ok(())
                },
            ),
            MachineCommand::Archive(args) => {
                archive_machine(&conn, args.id)?;
                emit_status(cli.json, "archived", args.id)
            }
            MachineCommand::Restore(args) => {
                restore_machine(&conn, args.id)?;
                emit_status(cli.json, "restored", args.id)
            }
        },
        Command::Exercise { command } => match command {
            ExerciseCommand::Add(args) => {
                let kind = ExerciseKind::from_str(&args.kind)?;
                let exercise = add_exercise(
                    &mut conn,
                    &args.name,
                    kind,
                    &args.tags,
                    args.description.as_deref(),
                )?;
                emit(cli.json, &exercise, || {
                    println!("added exercise {} ({})", exercise.id, exercise.name);
                    Ok(())
                })
            }
            ExerciseCommand::List(args) => {
                let kind = args
                    .kind
                    .as_deref()
                    .map(ExerciseKind::from_str)
                    .transpose()?;
                let exercises =
                    list_exercises(&conn, args.tag.as_deref(), kind, args.include_archived)?;
                emit(cli.json, &exercises, || {
                    for exercise in &exercises {
                        let tags = if exercise.tags.is_empty() {
                            String::new()
                        } else {
                            format!(" tags={}", exercise.tags.join(","))
                        };
                        println!(
                            "{}: {} [{}]{}{}",
                            exercise.id,
                            exercise.name,
                            exercise.kind.as_str(),
                            tags,
                            archived_suffix(&exercise.archived_at)
                        );
                    }
                    Ok(())
                })
            }
            ExerciseCommand::Update(args) => {
                let kind = args
                    .kind
                    .as_deref()
                    .map(ExerciseKind::from_str)
                    .transpose()?;
                let replace_tags = if args.clear_tags || !args.tags.is_empty() {
                    Some(args.tags.as_slice())
                } else {
                    None
                };
                let exercise = update_exercise(
                    &mut conn,
                    args.id,
                    args.name.as_deref(),
                    kind,
                    replace_tags,
                    args.description.as_deref(),
                )?;
                emit(cli.json, &exercise, || {
                    println!("updated exercise {} ({})", exercise.id, exercise.name);
                    Ok(())
                })
            }
            ExerciseCommand::Archive(args) => {
                archive_exercise(&conn, args.id)?;
                emit_status(cli.json, "archived", args.id)
            }
            ExerciseCommand::Restore(args) => {
                restore_exercise(&conn, args.id)?;
                emit_status(cli.json, "restored", args.id)
            }
        },
        Command::Session { command } => match command {
            SessionCommand::Start(args) => {
                let session = start_session(&conn, args.gym, args.notes.as_deref())?;
                emit(cli.json, &session, || {
                    println!("started session {} at {}", session.id, session.gym_name);
                    Ok(())
                })
            }
            SessionCommand::Current => {
                let session = current_session(&conn)?;
                emit(cli.json, &session, || {
                    if let Some(session) = session.as_ref() {
                        println!(
                            "active session {} at {} started {}",
                            session.id, session.gym_name, session.started_at
                        );
                    } else {
                        println!("no active session");
                    }
                    Ok(())
                })
            }
            SessionCommand::Finish(args) => {
                let session = finish_session(&conn, args.notes.as_deref())?;
                emit(cli.json, &session, || {
                    println!("finished session {}", session.id);
                    Ok(())
                })
            }
            SessionCommand::Cancel => {
                cancel_session(&conn)?;
                emit_status(cli.json, "cancelled", 0)
            }
        },
        Command::Log { command } => match command {
            LogCommand::Exercise(args) => {
                let result = log_exercise(
                    &conn,
                    args.exercise_id,
                    args.machine,
                    args.notes.as_deref(),
                    args.history,
                )?;
                emit(cli.json, &result, || print_log_exercise(&result))
            }
            LogCommand::Set(args) => {
                let entry_id = args
                    .exercise_entry
                    .map(Ok)
                    .unwrap_or_else(|| latest_session_exercise_id(&conn))?;
                let unit = args.unit.as_deref().map(WeightUnit::from_str).transpose()?;
                let set = log_set(&conn, entry_id, args.reps, args.weight, unit)?;
                emit(cli.json, &set, || print_set(&set))
            }
        },
    }
}

fn emit<T: Serialize, F: FnOnce() -> Result<()>>(json: bool, value: &T, text: F) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    } else {
        text()
    }
}

fn emit_status(json: bool, status: &'static str, id: i64) -> Result<()> {
    let value = Status {
        status,
        id: if id == 0 { None } else { Some(id) },
    };
    emit(json, &value, || {
        println!("{status}");
        Ok(())
    })
}

fn print_log_exercise(result: &LogExerciseResult) -> Result<()> {
    println!(
        "added session exercise {}: {}",
        result.entry.id, result.entry.exercise_name
    );
    if result.history.is_empty() {
        println!("previous: no prior completed sessions");
        return Ok(());
    }

    println!("previous:");
    for history in &result.history {
        print_history(history)?;
    }
    Ok(())
}

fn print_history(history: &HistoryEntry) -> Result<()> {
    let machine = history
        .machine_name
        .as_ref()
        .map(|name| format!(" on {name}"))
        .unwrap_or_default();
    println!(
        "- session {} at {} on {}{}",
        history.session_id, history.gym_name, history.session_date, machine
    );
    if history.sets.is_empty() {
        println!("  no sets recorded");
    } else {
        for set in &history.sets {
            println!("  {}. {}", set.position, set_label(set)?);
        }
    }
    Ok(())
}

fn print_set(set: &SetEntry) -> Result<()> {
    println!("added set {}: {}", set.position, set_label(set)?);
    Ok(())
}

fn set_label(set: &SetEntry) -> Result<String> {
    if set.reps <= 0 {
        bail!("invalid reps in stored set");
    }
    let label = match (set.weight_value, set.weight_unit) {
        (Some(weight), Some(unit)) => format!("{weight}{} x {}", unit.as_str(), set.reps),
        (None, None) => format!("bodyweight x {}", set.reps),
        _ => bail!("invalid stored set weight/unit pair"),
    };
    Ok(label)
}

fn archived_suffix(archived_at: &Option<String>) -> &'static str {
    if archived_at.is_some() {
        " archived"
    } else {
        ""
    }
}
