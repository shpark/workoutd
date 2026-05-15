use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExerciseKind {
    Freeweight,
    Machine,
    Calisthenics,
}

impl ExerciseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Freeweight => "freeweight",
            Self::Machine => "machine",
            Self::Calisthenics => "calisthenics",
        }
    }
}

impl FromStr for ExerciseKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "freeweight" => Ok(Self::Freeweight),
            "machine" => Ok(Self::Machine),
            "calisthenics" => Ok(Self::Calisthenics),
            _ => bail!("exercise kind must be one of: freeweight, machine, calisthenics"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WeightUnit {
    Kg,
    Lb,
}

impl WeightUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Kg => "kg",
            Self::Lb => "lb",
        }
    }
}

impl FromStr for WeightUnit {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "kg" => Ok(Self::Kg),
            "lb" => Ok(Self::Lb),
            _ => bail!("weight unit must be one of: kg, lb"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Gym {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Machine {
    pub id: i64,
    pub gym_id: i64,
    pub name: String,
    pub machine_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub settings_notes: Option<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Exercise {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub kind: ExerciseKind,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: i64,
    pub gym_id: i64,
    pub gym_name: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionExercise {
    pub id: i64,
    pub session_id: i64,
    pub exercise_id: i64,
    pub exercise_name: String,
    pub kind: ExerciseKind,
    pub machine_id: Option<i64>,
    pub machine_name: Option<String>,
    pub position: i64,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct SetEntry {
    pub id: i64,
    pub session_exercise_id: i64,
    pub position: i64,
    pub weight_value: Option<f64>,
    pub weight_unit: Option<WeightUnit>,
    pub reps: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub session_id: i64,
    pub session_date: String,
    pub gym_name: String,
    pub machine_name: Option<String>,
    pub sets: Vec<SetEntry>,
}

#[derive(Debug, Serialize)]
pub struct LogExerciseResult {
    pub entry: SessionExercise,
    pub history: Vec<HistoryEntry>,
}

pub fn default_db_path() -> Result<PathBuf> {
    if let Ok(path) = env::var("WORKOUTD_DB") {
        return Ok(PathBuf::from(path));
    }

    let base = if let Ok(path) = env::var("XDG_DATA_HOME") {
        PathBuf::from(path)
    } else {
        let home = env::var("HOME").context("HOME is not set and WORKOUTD_DB was not provided")?;
        PathBuf::from(home).join(".local/share")
    };

    Ok(base.join("workoutd/workoutd.sqlite3"))
}

pub fn open_database(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    ensure_schema(&conn)?;
    Ok(conn)
}

pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS gyms (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            address TEXT,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT
        );

        CREATE TABLE IF NOT EXISTS machines (
            id INTEGER PRIMARY KEY,
            gym_id INTEGER NOT NULL REFERENCES gyms(id),
            name TEXT NOT NULL,
            machine_type TEXT NOT NULL,
            brand TEXT,
            model TEXT,
            settings_notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT
        );

        CREATE TABLE IF NOT EXISTS exercises (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL CHECK (kind IN ('freeweight', 'machine', 'calisthenics')),
            description TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            archived_at TEXT
        );

        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS exercise_tags (
            exercise_id INTEGER NOT NULL REFERENCES exercises(id) ON DELETE CASCADE,
            tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (exercise_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY,
            gym_id INTEGER NOT NULL REFERENCES gyms(id),
            started_at TEXT NOT NULL DEFAULT (datetime('now')),
            finished_at TEXT,
            notes TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS one_active_session
            ON sessions((1))
            WHERE finished_at IS NULL;

        CREATE TABLE IF NOT EXISTS session_exercises (
            id INTEGER PRIMARY KEY,
            session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            exercise_id INTEGER NOT NULL REFERENCES exercises(id),
            machine_id INTEGER REFERENCES machines(id),
            position INTEGER NOT NULL,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(session_id, position)
        );

        CREATE TABLE IF NOT EXISTS sets (
            id INTEGER PRIMARY KEY,
            session_exercise_id INTEGER NOT NULL REFERENCES session_exercises(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            weight_value REAL,
            weight_unit TEXT CHECK (weight_unit IN ('kg', 'lb')),
            reps INTEGER NOT NULL CHECK (reps > 0),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(session_exercise_id, position),
            CHECK ((weight_value IS NULL AND weight_unit IS NULL) OR (weight_value IS NOT NULL AND weight_unit IS NOT NULL))
        );
        "#,
    )?;
    Ok(())
}

pub fn health_check(conn: &Connection) -> Result<()> {
    conn.query_row("SELECT 1", [], |_| Ok(()))?;
    Ok(())
}

pub fn add_gym(
    conn: &Connection,
    name: &str,
    address: Option<&str>,
    notes: Option<&str>,
) -> Result<Gym> {
    require_text(name, "gym name")?;
    let slug = unique_slug(conn, "gyms", name)?;
    conn.execute(
        "INSERT INTO gyms (name, slug, address, notes) VALUES (?1, ?2, ?3, ?4)",
        params![
            name.trim(),
            slug,
            empty_to_none(address),
            empty_to_none(notes)
        ],
    )?;
    get_gym(conn, conn.last_insert_rowid())
}

pub fn list_gyms(conn: &Connection, include_archived: bool) -> Result<Vec<Gym>> {
    let sql = if include_archived {
        "SELECT id, name, slug, address, notes, created_at, archived_at FROM gyms ORDER BY name"
    } else {
        "SELECT id, name, slug, address, notes, created_at, archived_at FROM gyms WHERE archived_at IS NULL ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], map_gym)?;
    collect_rows(rows)
}

pub fn archive_gym(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "gyms", id, true)
}

pub fn restore_gym(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "gyms", id, false)
}

pub fn add_machine(
    conn: &Connection,
    gym_id: i64,
    name: &str,
    machine_type: &str,
    brand: Option<&str>,
    model: Option<&str>,
    settings_notes: Option<&str>,
) -> Result<Machine> {
    require_active_gym(conn, gym_id)?;
    require_text(name, "machine name")?;
    require_text(machine_type, "machine type")?;
    conn.execute(
        "INSERT INTO machines (gym_id, name, machine_type, brand, model, settings_notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            gym_id,
            name.trim(),
            machine_type.trim(),
            empty_to_none(brand),
            empty_to_none(model),
            empty_to_none(settings_notes)
        ],
    )?;
    get_machine(conn, conn.last_insert_rowid())
}

pub fn list_machines(
    conn: &Connection,
    gym_id: i64,
    include_archived: bool,
) -> Result<Vec<Machine>> {
    require_existing_gym(conn, gym_id)?;
    let sql = if include_archived {
        "SELECT id, gym_id, name, machine_type, brand, model, settings_notes, created_at, archived_at
         FROM machines WHERE gym_id = ?1 ORDER BY name"
    } else {
        "SELECT id, gym_id, name, machine_type, brand, model, settings_notes, created_at, archived_at
         FROM machines WHERE gym_id = ?1 AND archived_at IS NULL ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([gym_id], map_machine)?;
    collect_rows(rows)
}

pub fn archive_machine(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "machines", id, true)
}

pub fn restore_machine(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "machines", id, false)
}

pub fn add_exercise(
    conn: &mut Connection,
    name: &str,
    kind: ExerciseKind,
    tags: &[String],
    description: Option<&str>,
) -> Result<Exercise> {
    require_text(name, "exercise name")?;
    let slug = unique_slug(conn, "exercises", name)?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO exercises (name, slug, kind, description) VALUES (?1, ?2, ?3, ?4)",
        params![name.trim(), slug, kind.as_str(), empty_to_none(description)],
    )?;
    let id = tx.last_insert_rowid();
    replace_tags_tx(&tx, id, tags)?;
    tx.commit()?;
    get_exercise(conn, id)
}

pub fn update_exercise(
    conn: &mut Connection,
    id: i64,
    name: Option<&str>,
    kind: Option<ExerciseKind>,
    replace_tags: Option<&[String]>,
    description: Option<&str>,
) -> Result<Exercise> {
    get_exercise(conn, id)?;
    if let Some(name) = name {
        require_text(name, "exercise name")?;
    }
    let tx = conn.transaction()?;
    if let Some(name) = name {
        tx.execute(
            "UPDATE exercises SET name = ?1 WHERE id = ?2",
            params![name.trim(), id],
        )?;
    }
    if let Some(kind) = kind {
        tx.execute(
            "UPDATE exercises SET kind = ?1 WHERE id = ?2",
            params![kind.as_str(), id],
        )?;
    }
    if let Some(description) = description {
        tx.execute(
            "UPDATE exercises SET description = ?1 WHERE id = ?2",
            params![empty_to_none(Some(description)), id],
        )?;
    }
    if let Some(tags) = replace_tags {
        replace_tags_tx(&tx, id, tags)?;
    }
    tx.commit()?;
    get_exercise(conn, id)
}

pub fn list_exercises(
    conn: &Connection,
    tag: Option<&str>,
    kind: Option<ExerciseKind>,
    include_archived: bool,
) -> Result<Vec<Exercise>> {
    let mut filters = Vec::new();
    if !include_archived {
        filters.push("e.archived_at IS NULL".to_string());
    }
    if tag.is_some() {
        filters.push("EXISTS (SELECT 1 FROM exercise_tags et JOIN tags t ON t.id = et.tag_id WHERE et.exercise_id = e.id AND t.name = ?1)".to_string());
    }
    if kind.is_some() {
        filters.push(format!("e.kind = ?{}", if tag.is_some() { 2 } else { 1 }));
    }

    let where_clause = if filters.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", filters.join(" AND "))
    };
    let sql = format!(
        "SELECT e.id, e.name, e.slug, e.kind, e.description, e.created_at, e.archived_at
         FROM exercises e{} ORDER BY e.name",
        where_clause
    );
    let mut stmt = conn.prepare(&sql)?;
    let ids: Vec<i64> = match (tag, kind) {
        (Some(tag), Some(kind)) => {
            collect_rows(stmt.query_map(params![tag, kind.as_str()], |row| row.get(0))?)?
        }
        (Some(tag), None) => collect_rows(stmt.query_map([tag], |row| row.get(0))?)?,
        (None, Some(kind)) => collect_rows(stmt.query_map([kind.as_str()], |row| row.get(0))?)?,
        (None, None) => collect_rows(stmt.query_map([], |row| row.get(0))?)?,
    };
    ids.into_iter().map(|id| get_exercise(conn, id)).collect()
}

pub fn archive_exercise(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "exercises", id, true)
}

pub fn restore_exercise(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "exercises", id, false)
}

pub fn start_session(conn: &Connection, gym_id: i64, notes: Option<&str>) -> Result<Session> {
    require_active_gym(conn, gym_id)?;
    conn.execute(
        "INSERT INTO sessions (gym_id, notes) VALUES (?1, ?2)",
        params![gym_id, empty_to_none(notes)],
    )
    .map_err(|err| {
        if is_unique_error(&err) {
            anyhow!("a session is already active")
        } else {
            anyhow!(err)
        }
    })?;
    get_session(conn, conn.last_insert_rowid())
}

pub fn current_session(conn: &Connection) -> Result<Option<Session>> {
    conn.query_row(
        "SELECT s.id, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         WHERE s.finished_at IS NULL",
        [],
        map_session,
    )
    .optional()
    .map_err(Into::into)
}

pub fn finish_session(conn: &Connection, notes: Option<&str>) -> Result<Session> {
    let session = current_session(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    if let Some(notes) = notes {
        conn.execute(
            "UPDATE sessions SET notes = ?1 WHERE id = ?2",
            params![empty_to_none(Some(notes)), session.id],
        )?;
    }
    conn.execute(
        "UPDATE sessions SET finished_at = datetime('now') WHERE id = ?1",
        [session.id],
    )?;
    get_session(conn, session.id)
}

pub fn cancel_session(conn: &Connection) -> Result<()> {
    let session = current_session(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", [session.id])?;
    Ok(())
}

pub fn log_exercise(
    conn: &Connection,
    exercise_id: i64,
    machine_id: Option<i64>,
    notes: Option<&str>,
    history_limit: usize,
) -> Result<LogExerciseResult> {
    let session = current_session(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    let exercise = get_exercise(conn, exercise_id)?;
    if exercise.archived_at.is_some() {
        bail!("exercise {} is archived", exercise_id);
    }

    match exercise.kind {
        ExerciseKind::Machine => {
            let machine_id =
                machine_id.ok_or_else(|| anyhow!("machine exercises require --machine"))?;
            let machine = get_machine(conn, machine_id)?;
            if machine.archived_at.is_some() {
                bail!("machine {} is archived", machine_id);
            }
            if machine.gym_id != session.gym_id {
                bail!(
                    "machine {} does not belong to active session gym {}",
                    machine_id,
                    session.gym_id
                );
            }
        }
        _ if machine_id.is_some() => bail!("--machine is only valid for machine exercises"),
        _ => {}
    }

    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM session_exercises WHERE session_id = ?1",
        [session.id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO session_exercises (session_id, exercise_id, machine_id, position, notes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session.id,
            exercise_id,
            machine_id,
            position,
            empty_to_none(notes)
        ],
    )?;
    let entry_id = conn.last_insert_rowid();
    Ok(LogExerciseResult {
        entry: get_session_exercise(conn, entry_id)?,
        history: previous_history(conn, exercise_id, session.id, history_limit)?,
    })
}

pub fn latest_session_exercise_id(conn: &Connection) -> Result<i64> {
    let session = current_session(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    conn.query_row(
        "SELECT id FROM session_exercises WHERE session_id = ?1 ORDER BY position DESC LIMIT 1",
        [session.id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| anyhow!("active session has no exercises yet"))
}

pub fn log_set(
    conn: &Connection,
    session_exercise_id: i64,
    reps: i64,
    weight_value: Option<f64>,
    weight_unit: Option<WeightUnit>,
) -> Result<SetEntry> {
    if reps <= 0 {
        bail!("reps must be greater than zero");
    }
    match (weight_value, weight_unit) {
        (Some(value), Some(_)) if value < 0.0 => bail!("weight cannot be negative"),
        (Some(_), Some(_)) | (None, None) => {}
        _ => bail!("weight and unit must be provided together"),
    }

    let active = current_session(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    let entry_session_id: i64 = conn
        .query_row(
            "SELECT session_id FROM session_exercises WHERE id = ?1",
            [session_exercise_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("session exercise {} not found", session_exercise_id))?;
    if entry_session_id != active.id {
        bail!("sets can only be added to the active session");
    }

    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM sets WHERE session_exercise_id = ?1",
        [session_exercise_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO sets (session_exercise_id, position, weight_value, weight_unit, reps)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session_exercise_id,
            position,
            weight_value,
            weight_unit.map(WeightUnit::as_str),
            reps
        ],
    )?;
    get_set(conn, conn.last_insert_rowid())
}

fn get_gym(conn: &Connection, id: i64) -> Result<Gym> {
    conn.query_row(
        "SELECT id, name, slug, address, notes, created_at, archived_at FROM gyms WHERE id = ?1",
        [id],
        map_gym,
    )
    .optional()?
    .ok_or_else(|| anyhow!("gym {} not found", id))
}

fn get_machine(conn: &Connection, id: i64) -> Result<Machine> {
    conn.query_row(
        "SELECT id, gym_id, name, machine_type, brand, model, settings_notes, created_at, archived_at
         FROM machines WHERE id = ?1",
        [id],
        map_machine,
    )
    .optional()?
    .ok_or_else(|| anyhow!("machine {} not found", id))
}

fn get_exercise(conn: &Connection, id: i64) -> Result<Exercise> {
    let mut exercise = conn
        .query_row(
            "SELECT id, name, slug, kind, description, created_at, archived_at FROM exercises WHERE id = ?1",
            [id],
            map_exercise_without_tags,
        )
        .optional()?
        .ok_or_else(|| anyhow!("exercise {} not found", id))?;
    exercise.tags = exercise_tags(conn, id)?;
    Ok(exercise)
}

fn get_session(conn: &Connection, id: i64) -> Result<Session> {
    conn.query_row(
        "SELECT s.id, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         WHERE s.id = ?1",
        [id],
        map_session,
    )
    .optional()?
    .ok_or_else(|| anyhow!("session {} not found", id))
}

fn get_session_exercise(conn: &Connection, id: i64) -> Result<SessionExercise> {
    conn.query_row(
        "SELECT se.id, se.session_id, se.exercise_id, e.name, e.kind, se.machine_id, m.name, se.position, se.notes, se.created_at
         FROM session_exercises se
         JOIN exercises e ON e.id = se.exercise_id
         LEFT JOIN machines m ON m.id = se.machine_id
         WHERE se.id = ?1",
        [id],
        map_session_exercise,
    )
    .optional()?
    .ok_or_else(|| anyhow!("session exercise {} not found", id))
}

fn get_set(conn: &Connection, id: i64) -> Result<SetEntry> {
    conn.query_row(
        "SELECT id, session_exercise_id, position, weight_value, weight_unit, reps, created_at
         FROM sets WHERE id = ?1",
        [id],
        map_set,
    )
    .optional()?
    .ok_or_else(|| anyhow!("set {} not found", id))
}

fn previous_history(
    conn: &Connection,
    exercise_id: i64,
    active_session_id: i64,
    limit: usize,
) -> Result<Vec<HistoryEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT se.id, s.id, s.started_at, g.name, m.name
         FROM session_exercises se
         JOIN sessions s ON s.id = se.session_id
         JOIN gyms g ON g.id = s.gym_id
         LEFT JOIN machines m ON m.id = se.machine_id
         WHERE se.exercise_id = ?1
           AND s.id != ?2
           AND s.finished_at IS NOT NULL
         ORDER BY s.started_at DESC, se.position DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![exercise_id, active_session_id, limit as i64],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    let mut entries = Vec::new();
    for row in rows {
        let (session_exercise_id, session_id, session_date, gym_name, machine_name) = row?;
        entries.push(HistoryEntry {
            session_id,
            session_date,
            gym_name,
            machine_name,
            sets: sets_for_entry(conn, session_exercise_id)?,
        });
    }
    Ok(entries)
}

fn sets_for_entry(conn: &Connection, session_exercise_id: i64) -> Result<Vec<SetEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_exercise_id, position, weight_value, weight_unit, reps, created_at
         FROM sets WHERE session_exercise_id = ?1 ORDER BY position",
    )?;
    let rows = stmt.query_map([session_exercise_id], map_set)?;
    collect_rows(rows)
}

fn exercise_tags(conn: &Connection, exercise_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name
         FROM tags t JOIN exercise_tags et ON et.tag_id = t.id
         WHERE et.exercise_id = ?1
         ORDER BY t.name",
    )?;
    let rows = stmt.query_map([exercise_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn replace_tags_tx(
    tx: &rusqlite::Transaction<'_>,
    exercise_id: i64,
    tags: &[String],
) -> Result<()> {
    tx.execute(
        "DELETE FROM exercise_tags WHERE exercise_id = ?1",
        [exercise_id],
    )?;
    for tag in normalized_tags(tags) {
        tx.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [&tag])?;
        let tag_id: i64 = tx.query_row("SELECT id FROM tags WHERE name = ?1", [&tag], |row| {
            row.get(0)
        })?;
        tx.execute(
            "INSERT OR IGNORE INTO exercise_tags (exercise_id, tag_id) VALUES (?1, ?2)",
            params![exercise_id, tag_id],
        )?;
    }
    Ok(())
}

fn normalized_tags(tags: &[String]) -> Vec<String> {
    let mut normalized = tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn require_active_gym(conn: &Connection, gym_id: i64) -> Result<()> {
    let archived_at: Option<String> = conn
        .query_row(
            "SELECT archived_at FROM gyms WHERE id = ?1",
            [gym_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("gym {} not found", gym_id))?;
    if archived_at.is_some() {
        bail!("gym {} is archived", gym_id);
    }
    Ok(())
}

fn require_existing_gym(conn: &Connection, gym_id: i64) -> Result<()> {
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM gyms WHERE id = ?1", [gym_id], |row| {
            row.get(0)
        })
        .optional()?;
    if exists.is_none() {
        bail!("gym {} not found", gym_id);
    }
    Ok(())
}

fn set_archive(conn: &Connection, table: &str, id: i64, archive: bool) -> Result<()> {
    let sql = if archive {
        format!("UPDATE {table} SET archived_at = datetime('now') WHERE id = ?1")
    } else {
        format!("UPDATE {table} SET archived_at = NULL WHERE id = ?1")
    };
    let changed = conn.execute(&sql, [id])?;
    if changed == 0 {
        bail!("{} {} not found", table.trim_end_matches('s'), id);
    }
    Ok(())
}

fn unique_slug(conn: &Connection, table: &str, value: &str) -> Result<String> {
    let base = slugify(value);
    let mut candidate = base.clone();
    for suffix in 2.. {
        let sql = format!("SELECT 1 FROM {table} WHERE slug = ?1");
        let exists: Option<i64> = conn
            .query_row(&sql, [&candidate], |row| row.get(0))
            .optional()?;
        if exists.is_none() {
            return Ok(candidate);
        }
        candidate = format!("{base}-{suffix}");
    }
    unreachable!()
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn require_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(())
}

fn empty_to_none(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn is_unique_error(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(error, _)
            if error.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn map_gym(row: &Row<'_>) -> rusqlite::Result<Gym> {
    Ok(Gym {
        id: row.get(0)?,
        name: row.get(1)?,
        slug: row.get(2)?,
        address: row.get(3)?,
        notes: row.get(4)?,
        created_at: row.get(5)?,
        archived_at: row.get(6)?,
    })
}

fn map_machine(row: &Row<'_>) -> rusqlite::Result<Machine> {
    Ok(Machine {
        id: row.get(0)?,
        gym_id: row.get(1)?,
        name: row.get(2)?,
        machine_type: row.get(3)?,
        brand: row.get(4)?,
        model: row.get(5)?,
        settings_notes: row.get(6)?,
        created_at: row.get(7)?,
        archived_at: row.get(8)?,
    })
}

fn map_exercise_without_tags(row: &Row<'_>) -> rusqlite::Result<Exercise> {
    let kind: String = row.get(3)?;
    Ok(Exercise {
        id: row.get(0)?,
        name: row.get(1)?,
        slug: row.get(2)?,
        kind: ExerciseKind::from_str(&kind).map_err(to_sql_error)?,
        description: row.get(4)?,
        tags: Vec::new(),
        created_at: row.get(5)?,
        archived_at: row.get(6)?,
    })
}

fn map_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        gym_id: row.get(1)?,
        gym_name: row.get(2)?,
        started_at: row.get(3)?,
        finished_at: row.get(4)?,
        notes: row.get(5)?,
    })
}

fn map_session_exercise(row: &Row<'_>) -> rusqlite::Result<SessionExercise> {
    let kind: String = row.get(4)?;
    Ok(SessionExercise {
        id: row.get(0)?,
        session_id: row.get(1)?,
        exercise_id: row.get(2)?,
        exercise_name: row.get(3)?,
        kind: ExerciseKind::from_str(&kind).map_err(to_sql_error)?,
        machine_id: row.get(5)?,
        machine_name: row.get(6)?,
        position: row.get(7)?,
        notes: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn map_set(row: &Row<'_>) -> rusqlite::Result<SetEntry> {
    let unit: Option<String> = row.get(4)?;
    Ok(SetEntry {
        id: row.get(0)?,
        session_exercise_id: row.get(1)?,
        position: row.get(2)?,
        weight_value: row.get(3)?,
        weight_unit: unit
            .as_deref()
            .map(WeightUnit::from_str)
            .transpose()
            .map_err(to_sql_error)?,
        reps: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn to_sql_error(err: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, err.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn one_active_session_is_enforced() {
        let conn = conn();
        let gym = add_gym(&conn, "Main Gym", None, None).unwrap();
        start_session(&conn, gym.id, None).unwrap();
        let err = start_session(&conn, gym.id, None).unwrap_err().to_string();
        assert!(err.contains("already active"));
    }

    #[test]
    fn machine_exercise_requires_machine_from_session_gym() {
        let conn = conn();
        let mut mutable = conn;
        let gym_a = add_gym(&mutable, "A", None, None).unwrap();
        let gym_b = add_gym(&mutable, "B", None, None).unwrap();
        let other_machine = add_machine(
            &mutable,
            gym_b.id,
            "Leg Press",
            "leg-press",
            None,
            None,
            None,
        )
        .unwrap();
        let exercise =
            add_exercise(&mut mutable, "Leg Press", ExerciseKind::Machine, &[], None).unwrap();
        start_session(&mutable, gym_a.id, None).unwrap();

        let missing = log_exercise(&mutable, exercise.id, None, None, 1)
            .unwrap_err()
            .to_string();
        assert!(missing.contains("require"));

        let wrong_gym = log_exercise(&mutable, exercise.id, Some(other_machine.id), None, 1)
            .unwrap_err()
            .to_string();
        assert!(wrong_gym.contains("does not belong"));
    }

    #[test]
    fn tags_can_be_queried() {
        let conn = conn();
        let mut mutable = conn;
        add_exercise(
            &mut mutable,
            "Bench Press",
            ExerciseKind::Freeweight,
            &["push".into(), "chest".into()],
            None,
        )
        .unwrap();

        let push = list_exercises(&mutable, Some("push"), None, false).unwrap();
        assert_eq!(push.len(), 1);
        assert_eq!(push[0].tags, vec!["chest", "push"]);
    }

    #[test]
    fn previous_history_excludes_active_session() {
        let conn = conn();
        let mut mutable = conn;
        let gym = add_gym(&mutable, "Main", None, None).unwrap();
        let exercise = add_exercise(
            &mut mutable,
            "Pull Up",
            ExerciseKind::Calisthenics,
            &[],
            None,
        )
        .unwrap();

        start_session(&mutable, gym.id, None).unwrap();
        let first = log_exercise(&mutable, exercise.id, None, None, 1).unwrap();
        log_set(&mutable, first.entry.id, 8, None, None).unwrap();
        finish_session(&mutable, None).unwrap();

        start_session(&mutable, gym.id, None).unwrap();
        let second = log_exercise(&mutable, exercise.id, None, None, 1).unwrap();
        assert_eq!(second.history.len(), 1);
        assert_eq!(second.history[0].sets[0].reps, 8);
    }

    #[test]
    fn weight_and_unit_are_validated_together() {
        let conn = conn();
        let mut mutable = conn;
        let gym = add_gym(&mutable, "Main", None, None).unwrap();
        let exercise =
            add_exercise(&mut mutable, "Squat", ExerciseKind::Freeweight, &[], None).unwrap();
        start_session(&mutable, gym.id, None).unwrap();
        let entry = log_exercise(&mutable, exercise.id, None, None, 1).unwrap();

        let err = log_set(&mutable, entry.entry.id, 5, Some(100.0), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("provided together"));

        let set = log_set(
            &mutable,
            entry.entry.id,
            5,
            Some(100.0),
            Some(WeightUnit::Kg),
        )
        .unwrap();
        assert_eq!(set.weight_unit, Some(WeightUnit::Kg));
    }
}
