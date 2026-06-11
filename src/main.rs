use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use workoutd::{
    add_exercise, add_gym, add_machine, add_machine_note, archive_exercise, archive_gym,
    archive_machine, archive_machine_note, cancel_session, current_session, default_db_path,
    delete_session, export_session, find_exercise_exact, find_gym_exact, find_machine_by_uuid,
    find_machine_uuid_for_gym, finish_session, import_session, latest_session_exercise_id,
    list_allowed_tags, list_exercises, list_gyms, list_machine_notes, list_machines,
    list_preset_machine_brands, list_sessions, log_exercise, log_set, open_database,
    recent_muscle_group_volume, restore_exercise, restore_gym, restore_machine,
    restore_machine_note, start_session, suggest_exercises, suggest_gyms, suggest_machines_for_gym,
    update_exercise, ExerciseKind, ExerciseSuggestion, GymSuggestion, HistoryEntry,
    LogExerciseResult, MachineLoadKind, MachineNote, MachineSuggestion, MuscleGroupVolumeSummary,
    Session, SessionExport, SetEntry, WeightUnit,
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
    #[command(about = "Register, list, archive, or restore gyms")]
    Gym {
        #[command(subcommand)]
        command: GymCommand,
    },
    #[command(about = "Register machines and manage machine notes")]
    Machine {
        #[command(subcommand)]
        command: MachineCommand,
    },
    #[command(about = "Register, list, update, archive, or restore exercises")]
    Exercise {
        #[command(subcommand)]
        command: ExerciseCommand,
    },
    #[command(about = "List allowed exercise tags")]
    Tag {
        #[command(subcommand)]
        command: TagCommand,
    },
    #[command(about = "Start, list, inspect, finish, cancel, or delete sessions")]
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    #[command(about = "Add exercises and sets to the active session")]
    Log {
        #[command(subcommand)]
        command: LogCommand,
    },
}

#[derive(Subcommand)]
enum GymCommand {
    #[command(about = "Register a gym")]
    Add(GymAdd),
    #[command(about = "List gyms")]
    List(ListArchived),
    #[command(about = "Soft-delete a gym")]
    Archive(IdArg),
    #[command(about = "Restore an archived gym")]
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
    #[command(about = "Register a physical machine at a gym")]
    Add(MachineAdd),
    #[command(about = "List machines at a gym")]
    List(MachineList),
    #[command(about = "Look up machine UUIDs by gym and search text")]
    Lookup(MachineLookup),
    #[command(about = "List preset machine brands")]
    Brand {
        #[command(subcommand)]
        command: MachineBrandCommand,
    },
    #[command(about = "Add, list, archive, or restore machine notes")]
    Note {
        #[command(subcommand)]
        command: MachineNoteCommand,
    },
    #[command(about = "Soft-delete a machine")]
    Archive(IdArg),
    #[command(about = "Restore an archived machine")]
    Restore(IdArg),
}

#[derive(Args)]
struct MachineAdd {
    #[arg(long)]
    gym: String,
    #[arg(long)]
    name: String,
    #[arg(long = "type")]
    machine_type: String,
    #[arg(long)]
    brand: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long = "load-kind")]
    load_kind: Option<String>,
    #[arg(long = "settings-notes")]
    settings_notes: Option<String>,
}

#[derive(Args)]
struct MachineList {
    #[arg(long)]
    gym: String,
    #[arg(long)]
    include_archived: bool,
}

#[derive(Args)]
struct MachineLookup {
    #[arg(long)]
    gym: String,
    query: String,
    #[arg(long, default_value_t = 5)]
    limit: usize,
}

#[derive(Subcommand)]
enum MachineBrandCommand {
    #[command(about = "Print preset machine brands")]
    List,
}

#[derive(Subcommand)]
enum MachineNoteCommand {
    #[command(about = "Add a durable note or setting hint to a machine")]
    Add(MachineNoteAdd),
    #[command(about = "List notes for a machine")]
    List(MachineNoteList),
    #[command(about = "Soft-delete a machine note")]
    Archive(IdArg),
    #[command(about = "Restore an archived machine note")]
    Restore(IdArg),
}

#[derive(Args)]
struct MachineNoteAdd {
    machine: String,
    #[arg(long)]
    note: String,
}

#[derive(Args)]
struct MachineNoteList {
    machine: String,
    #[arg(long)]
    include_archived: bool,
}

#[derive(Subcommand)]
enum ExerciseCommand {
    #[command(about = "Register an exercise; similar names require --force")]
    Add(ExerciseAdd),
    #[command(about = "List exercises, optionally filtered by allowed tag or kind")]
    List(ExerciseList),
    #[command(about = "Update an exercise and optionally replace its tags")]
    Update(ExerciseUpdate),
    #[command(about = "Soft-delete an exercise")]
    Archive(IdArg),
    #[command(about = "Restore an archived exercise")]
    Restore(IdArg),
}

#[derive(Subcommand)]
enum TagCommand {
    #[command(about = "Print the preset exercise tag allowlist")]
    List,
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
    /// Add even when similar exercise names already exist.
    #[arg(long)]
    force: bool,
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
    #[command(about = "Start a new session at a gym")]
    Start(SessionStart),
    #[command(about = "List sessions, optionally filtered by date")]
    List(SessionList),
    #[command(about = "Summarize set volume by muscle group for recent completed sessions")]
    Volume(SessionVolume),
    #[command(about = "Show the active session")]
    Current,
    #[command(about = "Export a full session as JSON")]
    Export(SessionExportArgs),
    #[command(about = "Import a session JSON export")]
    Import(SessionImportArgs),
    #[command(about = "Finish the active session")]
    Finish(SessionFinish),
    #[command(about = "Delete the active unfinished session")]
    Cancel,
    #[command(about = "Delete any session by UUID")]
    Delete(SessionDelete),
}

#[derive(Args)]
struct SessionStart {
    #[arg(long)]
    gym: String,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Args)]
struct SessionList {
    /// Filter sessions by started date in YYYY-MM-DD format.
    #[arg(long)]
    date: Option<String>,
}

#[derive(Args)]
struct SessionVolume {
    /// Number of recent completed sessions to summarize.
    #[arg(long, default_value_t = 3)]
    sessions: usize,
}

#[derive(Args)]
struct SessionExportArgs {
    /// Session UUID from `workoutd session list` or session JSON output.
    session_id: String,
}

#[derive(Args)]
struct SessionImportArgs {
    /// Path to JSON created by `workoutd session export`.
    path: PathBuf,
}

#[derive(Args)]
struct SessionFinish {
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Args)]
struct SessionDelete {
    /// Session UUID from `workoutd session list` or session JSON output.
    session_id: String,
    /// Required confirmation for deleting a session and its logged exercises/sets.
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand)]
enum LogCommand {
    #[command(about = "Add an exercise to the active session; fuzzy matches only suggest")]
    Exercise(LogExercise),
    #[command(about = "Add a set to a session exercise")]
    Set(LogSet),
}

#[derive(Args)]
struct LogExercise {
    /// Exercise id, exact name, or exact slug. Non-exact matches print suggestions and abort.
    exercise: String,
    /// Machine UUID, scoped to the active session gym. Use `workoutd machine lookup` to find it.
    #[arg(long)]
    machine: Option<String>,
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
                let gym = resolve_active_gym_or_suggest(&conn, &args.gym)?;
                let load_kind = args
                    .load_kind
                    .as_deref()
                    .map(MachineLoadKind::from_str)
                    .transpose()?;
                let machine = add_machine(
                    &conn,
                    gym.id,
                    &args.name,
                    &args.machine_type,
                    args.brand.as_deref(),
                    args.model.as_deref(),
                    load_kind,
                    args.settings_notes.as_deref(),
                )?;
                emit(cli.json, &machine, || {
                    println!("added machine {} ({})", machine.uuid, machine.name);
                    Ok(())
                })
            }
            MachineCommand::List(args) => {
                let gym = resolve_existing_gym_or_suggest(&conn, &args.gym)?;
                let machines = list_machines(&conn, gym.id, args.include_archived)?;
                emit(cli.json, &machines, || {
                    for machine in &machines {
                        let load_kind = machine
                            .load_kind
                            .map(|load_kind| format!(" {}", load_kind.as_str()))
                            .unwrap_or_default();
                        println!(
                            "{}: {} [{}{}]{}",
                            machine.uuid,
                            machine.name,
                            machine.machine_type,
                            load_kind,
                            archived_suffix(&machine.archived_at)
                        );
                    }
                    Ok(())
                })
            }
            MachineCommand::Lookup(args) => {
                let gym = resolve_existing_gym_or_suggest(&conn, &args.gym)?;
                let machines = suggest_machines_for_gym(&conn, gym.id, &args.query, args.limit)?;
                emit(cli.json, &machines, || print_machine_suggestions(&machines))
            }
            MachineCommand::Brand { command } => match command {
                MachineBrandCommand::List => {
                    let brands = list_preset_machine_brands();
                    emit(cli.json, &brands, || {
                        for brand in &brands {
                            println!("{brand}");
                        }
                        Ok(())
                    })
                }
            },
            MachineCommand::Note { command } => match command {
                MachineNoteCommand::Add(args) => {
                    let machine = resolve_machine_uuid(&conn, &args.machine)?;
                    let note = add_machine_note(&conn, machine.id, &args.note)?;
                    emit(cli.json, &note, || {
                        println!(
                            "added machine note {} for machine {}",
                            note.id, machine.uuid
                        );
                        Ok(())
                    })
                }
                MachineNoteCommand::List(args) => {
                    let machine = resolve_machine_uuid(&conn, &args.machine)?;
                    let notes = list_machine_notes(&conn, machine.id, args.include_archived)?;
                    emit(cli.json, &notes, || print_machine_notes(&notes))
                }
                MachineNoteCommand::Archive(args) => {
                    archive_machine_note(&conn, args.id)?;
                    emit_status(cli.json, "archived", args.id)
                }
                MachineNoteCommand::Restore(args) => {
                    restore_machine_note(&conn, args.id)?;
                    emit_status(cli.json, "restored", args.id)
                }
            },
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
                if let Some(existing) = find_exercise_exact(&conn, &args.name)? {
                    bail!(
                        "exercise '{}' already exists as id {}",
                        args.name,
                        existing.id
                    );
                }
                let suggestions = suggest_exercises(&conn, &args.name, 5)?;
                if !args.force && !suggestions.is_empty() {
                    bail!(
                        "similar exercises already exist; use --force to add anyway\n{}",
                        format_exercise_suggestions(&suggestions)
                    );
                }
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
        Command::Tag { command } => match command {
            TagCommand::List => {
                let tags = list_allowed_tags();
                emit(cli.json, &tags, || {
                    for tag in &tags {
                        println!("{tag}");
                    }
                    Ok(())
                })
            }
        },
        Command::Session { command } => match command {
            SessionCommand::Start(args) => {
                let gym = resolve_active_gym_or_suggest(&conn, &args.gym)?;
                let session = start_session(&conn, gym.id, args.notes.as_deref())?;
                emit(cli.json, &session, || {
                    println!("started session {} at {}", session.id, session.gym_name);
                    Ok(())
                })
            }
            SessionCommand::List(args) => {
                let sessions = list_sessions(&conn, args.date.as_deref())?;
                emit(cli.json, &sessions, || print_sessions(&sessions))
            }
            SessionCommand::Volume(args) => {
                let summary = recent_muscle_group_volume(&conn, args.sessions)?;
                emit(cli.json, &summary, || print_muscle_group_volume(&summary))
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
            SessionCommand::Export(args) => {
                let exported = export_session(&conn, &args.session_id)?;
                emit(true, &exported, || Ok(()))
            }
            SessionCommand::Import(args) => {
                let input = fs::read_to_string(&args.path)?;
                let exported: SessionExport = serde_json::from_str(&input)?;
                let imported = import_session(&mut conn, &exported)?;
                emit(cli.json, &imported, || {
                    println!(
                        "imported session {} at {} started {}",
                        imported.session.id, imported.session.gym_name, imported.session.started_at
                    );
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
            SessionCommand::Delete(args) => {
                if !args.yes {
                    bail!("deleting a session requires --yes");
                }
                let session = delete_session(&conn, &args.session_id)?;
                emit(cli.json, &session, || {
                    println!(
                        "deleted session {} at {} started {}",
                        session.id, session.gym_name, session.started_at
                    );
                    Ok(())
                })
            }
        },
        Command::Log { command } => match command {
            LogCommand::Exercise(args) => {
                let exercise = resolve_exercise_or_suggest(&conn, &args.exercise)?;
                let machine_id =
                    resolve_machine_for_log(&conn, &exercise.kind, args.machine.as_deref())?;
                let result = log_exercise(
                    &conn,
                    exercise.id,
                    machine_id,
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

fn resolve_active_gym_or_suggest(
    conn: &rusqlite::Connection,
    query: &str,
) -> Result<workoutd::Gym> {
    if let Some(gym) = find_gym_exact(conn, query, true)? {
        if gym.archived_at.is_some() {
            bail!("gym '{}' is archived", gym.name);
        }
        return Ok(gym);
    }
    let suggestions = suggest_gyms(conn, query, false, 5)?;
    if suggestions.is_empty() {
        bail!("gym '{}' not found", query);
    }
    bail!(
        "gym '{}' not found; similar gyms:\n{}",
        query,
        format_gym_suggestions(&suggestions)
    );
}

fn resolve_existing_gym_or_suggest(
    conn: &rusqlite::Connection,
    query: &str,
) -> Result<workoutd::Gym> {
    if let Some(gym) = find_gym_exact(conn, query, true)? {
        return Ok(gym);
    }
    let suggestions = suggest_gyms(conn, query, true, 5)?;
    if suggestions.is_empty() {
        bail!("gym '{}' not found", query);
    }
    bail!(
        "gym '{}' not found; similar gyms:\n{}",
        query,
        format_gym_suggestions(&suggestions)
    );
}

fn resolve_exercise_or_suggest(
    conn: &rusqlite::Connection,
    query: &str,
) -> Result<workoutd::Exercise> {
    if let Some(exercise) = find_exercise_exact(conn, query)? {
        return Ok(exercise);
    }
    let suggestions = suggest_exercises(conn, query, 5)?;
    if suggestions.is_empty() {
        bail!("exercise '{}' not found", query);
    }
    bail!(
        "exercise '{}' not found; similar exercises:\n{}",
        query,
        format_exercise_suggestions(&suggestions)
    );
}

fn resolve_machine_for_log(
    conn: &rusqlite::Connection,
    kind: &ExerciseKind,
    query: Option<&str>,
) -> Result<Option<i64>> {
    let Some(query) = query else {
        if *kind == ExerciseKind::Machine {
            let session =
                current_session(conn)?.ok_or_else(|| anyhow::anyhow!("no active session"))?;
            let machines = suggest_machines_for_gym(conn, session.gym_id, "", 10)?;
            if machines.is_empty() {
                bail!(
                    "machine exercises require --machine, and this gym has no registered machines"
                );
            }
            bail!(
                "machine exercises require --machine; available machines for {}:\n{}",
                session.gym_name,
                format_machine_suggestions(&machines)
            );
        }
        return Ok(None);
    };

    if *kind != ExerciseKind::Machine {
        bail!("--machine is only valid for machine exercises");
    }

    let session = current_session(conn)?.ok_or_else(|| anyhow::anyhow!("no active session"))?;
    let machine = match find_machine_uuid_for_gym(conn, session.gym_id, query) {
        Ok(Some(machine)) => machine,
        Ok(None) => {
            bail!(
                "machine UUID '{}' not found in active gym {}; run `workoutd machine lookup --gym \"{}\" QUERY` to find a machine UUID",
                query,
                session.gym_name,
                session.gym_name
            );
        }
        Err(err) => {
            bail!(
                "{}; --machine requires a machine UUID from `workoutd machine lookup --gym \"{}\" QUERY`",
                err,
                session.gym_name
            );
        }
    };
    Ok(Some(machine.id))
}

fn resolve_machine_uuid(conn: &rusqlite::Connection, query: &str) -> Result<workoutd::Machine> {
    find_machine_by_uuid(conn, query).map_err(|err| {
        anyhow::anyhow!(
            "{}; machine notes require a machine UUID from `workoutd machine lookup --gym GYM QUERY`",
            err
        )
    })
}

fn print_machine_suggestions(suggestions: &[MachineSuggestion]) -> Result<()> {
    if suggestions.is_empty() {
        println!("no machines");
        return Ok(());
    }
    println!("{}", format_machine_suggestions(suggestions));
    Ok(())
}

fn format_exercise_suggestions(suggestions: &[ExerciseSuggestion]) -> String {
    suggestions
        .iter()
        .map(|exercise| {
            format!(
                "- {}: {} [{}]",
                exercise.id,
                exercise.name,
                exercise.kind.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_machine_suggestions(suggestions: &[MachineSuggestion]) -> String {
    suggestions
        .iter()
        .map(|machine| {
            let brand = machine
                .brand
                .as_ref()
                .map(|brand| format!(" brand={brand}"))
                .unwrap_or_default();
            let model = machine
                .model
                .as_ref()
                .map(|model| format!(" model={model}"))
                .unwrap_or_default();
            let load_kind = machine
                .load_kind
                .map(|load_kind| format!(" load_kind={}", load_kind.as_str()))
                .unwrap_or_default();
            format!(
                "- {}: {} [{}]{}{}{}",
                machine.uuid, machine.name, machine.machine_type, brand, model, load_kind
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_gym_suggestions(suggestions: &[GymSuggestion]) -> String {
    suggestions
        .iter()
        .map(|gym| format!("- {}: {} ({})", gym.id, gym.name, gym.slug))
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_machine_notes(notes: &[MachineNote]) -> Result<()> {
    if notes.is_empty() {
        println!("no machine notes");
        return Ok(());
    }
    for note in notes {
        println!(
            "{}: {}{}",
            note.id,
            note.note,
            archived_suffix(&note.archived_at)
        );
    }
    Ok(())
}

fn print_sessions(sessions: &[Session]) -> Result<()> {
    if sessions.is_empty() {
        println!("no sessions");
        return Ok(());
    }
    for session in sessions {
        let state = if session.finished_at.is_some() {
            "finished"
        } else {
            "active"
        };
        println!(
            "{}: {} started {} ({})",
            session.id, session.gym_name, session.started_at, state
        );
    }
    Ok(())
}

fn print_muscle_group_volume(summary: &MuscleGroupVolumeSummary) -> Result<()> {
    if summary.sessions.is_empty() {
        println!("no completed sessions");
        return Ok(());
    }

    println!(
        "volume summary for latest {} completed sessions ({} found):",
        summary.session_limit,
        summary.sessions.len()
    );
    println!("sessions:");
    for session in &summary.sessions {
        println!(
            "- {} at {} started {}",
            session.id, session.gym_name, session.started_at
        );
    }

    println!("muscle groups:");
    if summary.muscle_groups.is_empty() {
        println!("- no sets recorded");
    } else {
        for group in &summary.muscle_groups {
            println!(
                "- {}: {} {}",
                group.muscle_group,
                group.sets,
                pluralize(group.sets, "set", "sets")
            );
        }
    }
    Ok(())
}

fn print_log_exercise(result: &LogExerciseResult) -> Result<()> {
    println!(
        "added session exercise {}: {}",
        result.entry.id, result.entry.exercise_name
    );
    if result.history.is_empty() {
        println!("previous: no prior completed sessions");
    } else {
        println!("previous:");
        for history in &result.history {
            print_history(history)?;
        }
    }

    if !result.machine_notes.is_empty() {
        println!("machine notes:");
        for note in &result.machine_notes {
            println!("- {} on {}: {}", note.id, note.created_at, note.note);
        }
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

fn pluralize(count: i64, singular: &'static str, plural: &'static str) -> &'static str {
    if count == 1 {
        singular
    } else {
        plural
    }
}
