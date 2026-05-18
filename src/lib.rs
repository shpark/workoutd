use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use uuid::Uuid;

pub const ALLOWED_TAGS: &[&str] = &[
    "biceps",
    "triceps",
    "quad",
    "glute",
    "hamstring",
    "back",
    "chest",
    "side delt",
    "rear delt",
    "front delt",
    "abs",
];

pub const PRESET_MACHINE_BRANDS: &[&str] = &[
    "Arsenal Strength",
    "Atlantis",
    "Cybex",
    "FreeMotion",
    "Hammer Strength",
    "Hoist",
    "Life Fitness",
    "Matrix",
    "Nautilus",
    "Newtech Wellness",
    "Panatta",
    "Precor",
    "Prime",
    "Rogue",
    "Star Trac",
    "Technogym",
    "True Fitness",
    "Watson Gym Equipment",
];

const MACHINE_BRAND_ALIASES: &[(&str, &str)] = &[
    ("Newtech", "Newtech Wellness"),
    ("New Tech", "Newtech Wellness"),
    ("Neutech", "Newtech Wellness"),
    ("Neutech Wellness", "Newtech Wellness"),
    ("Watson", "Watson Gym Equipment"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MachineLoadKind {
    PlateLoaded,
    PinLoaded,
}

impl MachineLoadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlateLoaded => "plate-loaded",
            Self::PinLoaded => "pin-loaded",
        }
    }
}

impl FromStr for MachineLoadKind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "plate-loaded" => Ok(Self::PlateLoaded),
            "pin-loaded" => Ok(Self::PinLoaded),
            _ => bail!("machine load kind must be one of: plate-loaded, pin-loaded"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Gym {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub address: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Machine {
    pub id: i64,
    pub uuid: String,
    pub gym_id: i64,
    pub name: String,
    pub machine_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub load_kind: Option<MachineLoadKind>,
    pub settings_notes: Option<String>,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MachineNote {
    pub id: i64,
    pub machine_id: i64,
    pub session_exercise_id: Option<i64>,
    pub note: String,
    pub created_at: String,
    pub archived_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
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

#[derive(Debug, Deserialize, Serialize)]
pub struct Session {
    pub id: String,
    pub gym_id: i64,
    pub gym_name: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionExercise {
    pub id: i64,
    pub session_id: String,
    pub exercise_id: i64,
    pub exercise_name: String,
    pub kind: ExerciseKind,
    pub machine_id: Option<i64>,
    pub machine_name: Option<String>,
    pub position: i64,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetEntry {
    pub id: i64,
    pub session_exercise_id: i64,
    pub position: i64,
    pub weight_value: Option<f64>,
    pub weight_unit: Option<WeightUnit>,
    pub reps: i64,
    pub created_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HistoryEntry {
    pub session_id: String,
    pub session_date: String,
    pub gym_name: String,
    pub machine_name: Option<String>,
    pub sets: Vec<SetEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionExport {
    pub session: Session,
    pub exercises: Vec<SessionExerciseExport>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionExerciseExport {
    pub entry: SessionExercise,
    pub machine: Option<Machine>,
    pub sets: Vec<SetEntry>,
    pub machine_notes: Vec<MachineNote>,
}

struct SessionRecord {
    row_id: i64,
    session: Session,
}

#[derive(Debug, Serialize)]
pub struct ExerciseSuggestion {
    pub id: i64,
    pub name: String,
    pub kind: ExerciseKind,
}

#[derive(Debug, Serialize)]
pub struct MachineSuggestion {
    pub uuid: String,
    pub name: String,
    pub machine_type: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub load_kind: Option<MachineLoadKind>,
}

#[derive(Debug, Serialize)]
pub struct GymSuggestion {
    pub id: i64,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Serialize)]
pub struct LogExerciseResult {
    pub entry: SessionExercise,
    pub history: Vec<HistoryEntry>,
    pub machine_notes: Vec<MachineNote>,
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
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            archived_at TEXT
        );

        CREATE TABLE IF NOT EXISTS machines (
            id INTEGER PRIMARY KEY,
            uuid TEXT NOT NULL UNIQUE,
            gym_id INTEGER NOT NULL REFERENCES gyms(id),
            name TEXT NOT NULL,
            machine_type TEXT NOT NULL,
            brand TEXT,
            model TEXT,
            load_kind TEXT CHECK (load_kind IN ('plate-loaded', 'pin-loaded')),
            settings_notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            archived_at TEXT
        );

        CREATE TABLE IF NOT EXISTS machine_notes (
            id INTEGER PRIMARY KEY,
            machine_id INTEGER NOT NULL REFERENCES machines(id),
            session_exercise_id INTEGER REFERENCES session_exercises(id) ON DELETE SET NULL,
            note TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            archived_at TEXT
        );

        CREATE TABLE IF NOT EXISTS exercises (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL CHECK (kind IN ('freeweight', 'machine', 'calisthenics')),
            description TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
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
            uuid TEXT NOT NULL UNIQUE,
            gym_id INTEGER NOT NULL REFERENCES gyms(id),
            started_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
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
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            UNIQUE(session_id, position)
        );

        CREATE TABLE IF NOT EXISTS sets (
            id INTEGER PRIMARY KEY,
            session_exercise_id INTEGER NOT NULL REFERENCES session_exercises(id) ON DELETE CASCADE,
            position INTEGER NOT NULL,
            weight_value REAL,
            weight_unit TEXT CHECK (weight_unit IN ('kg', 'lb')),
            reps INTEGER NOT NULL CHECK (reps > 0),
            created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
            UNIQUE(session_exercise_id, position),
            CHECK ((weight_value IS NULL AND weight_unit IS NULL) OR (weight_value IS NOT NULL AND weight_unit IS NOT NULL))
        );
        "#,
    )?;
    ensure_session_uuid_column(conn)?;
    ensure_machine_uuid_column(conn)?;
    ensure_machine_load_kind_column(conn)?;
    seed_allowed_tags(conn)?;
    Ok(())
}

pub fn health_check(conn: &Connection) -> Result<()> {
    conn.query_row("SELECT 1", [], |_| Ok(()))?;
    Ok(())
}

fn ensure_session_uuid_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(sessions)")?;
    let columns = collect_rows(stmt.query_map([], |row| row.get::<_, String>(1))?)?;
    if !columns.iter().any(|column| column == "uuid") {
        conn.execute("ALTER TABLE sessions ADD COLUMN uuid TEXT", [])?;
    }

    let mut stmt = conn.prepare("SELECT id FROM sessions WHERE uuid IS NULL OR uuid = ''")?;
    let missing_ids = collect_rows(stmt.query_map([], |row| row.get::<_, i64>(0))?)?;
    for id in missing_ids {
        conn.execute(
            "UPDATE sessions SET uuid = ?1 WHERE id = ?2",
            params![Uuid::new_v4().to_string(), id],
        )?;
    }

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS sessions_uuid_unique ON sessions(uuid)",
        [],
    )?;
    Ok(())
}

fn ensure_machine_uuid_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(machines)")?;
    let columns = collect_rows(stmt.query_map([], |row| row.get::<_, String>(1))?)?;
    if !columns.iter().any(|column| column == "uuid") {
        conn.execute("ALTER TABLE machines ADD COLUMN uuid TEXT", [])?;
    }

    let mut stmt = conn.prepare("SELECT id FROM machines WHERE uuid IS NULL OR uuid = ''")?;
    let missing_ids = collect_rows(stmt.query_map([], |row| row.get::<_, i64>(0))?)?;
    for id in missing_ids {
        conn.execute(
            "UPDATE machines SET uuid = ?1 WHERE id = ?2",
            params![Uuid::new_v4().to_string(), id],
        )?;
    }

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS machines_uuid_unique ON machines(uuid)",
        [],
    )?;
    Ok(())
}

fn ensure_machine_load_kind_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(machines)")?;
    let columns = collect_rows(stmt.query_map([], |row| row.get::<_, String>(1))?)?;
    if !columns.iter().any(|column| column == "load_kind") {
        conn.execute("ALTER TABLE machines ADD COLUMN load_kind TEXT", [])?;
    }
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
        "INSERT INTO gyms (name, slug, address, notes, created_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now','localtime'))",
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

pub fn find_gym_exact(
    conn: &Connection,
    query: &str,
    include_archived: bool,
) -> Result<Option<Gym>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        bail!("gym cannot be empty");
    }
    if let Ok(id) = trimmed.parse::<i64>() {
        return match get_gym(conn, id) {
            Ok(gym) if include_archived || gym.archived_at.is_none() => Ok(Some(gym)),
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        };
    }
    let slug = slugify(trimmed);
    let archived_filter = if include_archived {
        ""
    } else {
        " AND archived_at IS NULL"
    };
    let sql = format!(
        "SELECT id FROM gyms
         WHERE (lower(name) = lower(?1) OR slug = ?2){archived_filter}
         ORDER BY id LIMIT 1",
    );
    let id = conn
        .query_row(&sql, params![trimmed, slug], |row| row.get::<_, i64>(0))
        .optional()?;
    id.map(|id| get_gym(conn, id)).transpose()
}

pub fn suggest_gyms(
    conn: &Connection,
    query: &str,
    include_archived: bool,
    limit: usize,
) -> Result<Vec<GymSuggestion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let trimmed = query.trim();
    let mut scored = Vec::new();
    for gym in list_gyms(conn, include_archived)? {
        let score = if trimmed.is_empty() {
            Some(1)
        } else if normalize_search(&gym.name) == normalize_search(trimmed)
            || normalize_search(&gym.slug) == normalize_search(trimmed)
        {
            None
        } else {
            fuzzy_score(trimmed, &gym.name).or_else(|| fuzzy_score(trimmed, &gym.slug))
        };
        if let Some(score) = score {
            scored.push((
                score,
                GymSuggestion {
                    id: gym.id,
                    name: gym.name,
                    slug: gym.slug,
                },
            ));
        }
    }
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, suggestion)| suggestion)
        .collect())
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
    load_kind: Option<MachineLoadKind>,
    settings_notes: Option<&str>,
) -> Result<Machine> {
    require_active_gym(conn, gym_id)?;
    require_text(name, "machine name")?;
    require_text(machine_type, "machine type")?;
    let uuid = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO machines (uuid, gym_id, name, machine_type, brand, model, load_kind, settings_notes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now','localtime'))",
        params![
            uuid,
            gym_id,
            name.trim(),
            machine_type.trim(),
            normalize_machine_brand(brand)?,
            empty_to_none(model),
            load_kind.map(MachineLoadKind::as_str),
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
        "SELECT id, uuid, gym_id, name, machine_type, brand, model, load_kind, settings_notes, created_at, archived_at
         FROM machines WHERE gym_id = ?1 ORDER BY name"
    } else {
        "SELECT id, uuid, gym_id, name, machine_type, brand, model, load_kind, settings_notes, created_at, archived_at
         FROM machines WHERE gym_id = ?1 AND archived_at IS NULL ORDER BY name"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([gym_id], map_machine)?;
    collect_rows(rows)
}

pub fn find_machine_by_uuid(conn: &Connection, uuid: &str) -> Result<Machine> {
    let uuid = normalize_machine_uuid(uuid)?;
    let id = conn
        .query_row(
            "SELECT id FROM machines WHERE uuid = ?1",
            [uuid.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("machine UUID {} not found", uuid))?;
    get_machine(conn, id)
}

pub fn list_preset_machine_brands() -> Vec<String> {
    PRESET_MACHINE_BRANDS
        .iter()
        .map(|brand| (*brand).to_string())
        .collect()
}

pub fn archive_machine(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "machines", id, true)
}

pub fn restore_machine(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "machines", id, false)
}

pub fn add_machine_note(conn: &Connection, machine_id: i64, note: &str) -> Result<MachineNote> {
    let machine = get_machine(conn, machine_id)?;
    if machine.archived_at.is_some() {
        bail!("machine {} is archived", machine_id);
    }
    require_text(note, "machine note")?;
    conn.execute(
        "INSERT INTO machine_notes (machine_id, note, created_at)
         VALUES (?1, ?2, datetime('now','localtime'))",
        params![machine_id, note.trim()],
    )?;
    get_machine_note(conn, conn.last_insert_rowid())
}

pub fn list_machine_notes(
    conn: &Connection,
    machine_id: i64,
    include_archived: bool,
) -> Result<Vec<MachineNote>> {
    get_machine(conn, machine_id)?;
    let sql = if include_archived {
        "SELECT id, machine_id, session_exercise_id, note, created_at, archived_at
         FROM machine_notes WHERE machine_id = ?1 ORDER BY created_at DESC, id DESC"
    } else {
        "SELECT id, machine_id, session_exercise_id, note, created_at, archived_at
         FROM machine_notes WHERE machine_id = ?1 AND archived_at IS NULL ORDER BY created_at DESC, id DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([machine_id], map_machine_note)?;
    collect_rows(rows)
}

pub fn archive_machine_note(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "machine_notes", id, true)
}

pub fn restore_machine_note(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "machine_notes", id, false)
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
        "INSERT INTO exercises (name, slug, kind, description, created_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now','localtime'))",
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
    let tag = tag.map(normalize_allowed_tag).transpose()?;
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
            collect_rows(stmt.query_map(params![tag.as_str(), kind.as_str()], |row| row.get(0))?)?
        }
        (Some(tag), None) => collect_rows(stmt.query_map([tag.as_str()], |row| row.get(0))?)?,
        (None, Some(kind)) => collect_rows(stmt.query_map([kind.as_str()], |row| row.get(0))?)?,
        (None, None) => collect_rows(stmt.query_map([], |row| row.get(0))?)?,
    };
    ids.into_iter().map(|id| get_exercise(conn, id)).collect()
}

pub fn list_allowed_tags() -> Vec<String> {
    ALLOWED_TAGS.iter().map(|tag| (*tag).to_string()).collect()
}

pub fn find_exercise_exact(conn: &Connection, query: &str) -> Result<Option<Exercise>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        bail!("exercise cannot be empty");
    }
    if let Ok(id) = trimmed.parse::<i64>() {
        return match get_exercise(conn, id) {
            Ok(exercise) if exercise.archived_at.is_none() => Ok(Some(exercise)),
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        };
    }
    let slug = slugify(trimmed);
    let id = conn
        .query_row(
            "SELECT id FROM exercises
             WHERE archived_at IS NULL AND (lower(name) = lower(?1) OR slug = ?2)
             ORDER BY id LIMIT 1",
            params![trimmed, slug],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    id.map(|id| get_exercise(conn, id)).transpose()
}

pub fn suggest_exercises(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<ExerciseSuggestion>> {
    let trimmed = query.trim();
    if trimmed.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare("SELECT id, name, kind FROM exercises WHERE archived_at IS NULL ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        let kind: String = row.get(2)?;
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            ExerciseKind::from_str(&kind).map_err(to_sql_error)?,
        ))
    })?;
    let mut scored = Vec::new();
    for row in rows {
        let (id, name, kind) = row?;
        if normalize_search(&name) == normalize_search(trimmed) {
            continue;
        }
        if let Some(score) = fuzzy_score(trimmed, &name) {
            scored.push((score, ExerciseSuggestion { id, name, kind }));
        }
    }
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, suggestion)| suggestion)
        .collect())
}

pub fn find_machine_uuid_for_gym(
    conn: &Connection,
    gym_id: i64,
    uuid: &str,
) -> Result<Option<Machine>> {
    let uuid = normalize_machine_uuid(uuid)?;
    let id = conn
        .query_row(
            "SELECT id FROM machines
             WHERE gym_id = ?1 AND archived_at IS NULL AND uuid = ?2
             ORDER BY id LIMIT 1",
            params![gym_id, uuid],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    id.map(|id| get_machine(conn, id)).transpose()
}

pub fn suggest_machines_for_gym(
    conn: &Connection,
    gym_id: i64,
    query: &str,
    limit: usize,
) -> Result<Vec<MachineSuggestion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let machines = list_machines(conn, gym_id, false)?;
    let trimmed = query.trim();
    let normalized_query = normalize_search(trimmed);
    let mut scored = Vec::new();
    for machine in machines {
        let score = if trimmed.is_empty() {
            Some(1)
        } else if machine.uuid == trimmed {
            Some(20_000)
        } else if normalize_search(&machine.name) == normalized_query {
            Some(19_000)
        } else {
            fuzzy_score(trimmed, &machine.name)
                .or_else(|| fuzzy_score(trimmed, &machine.machine_type))
                .or_else(|| {
                    machine
                        .brand
                        .as_deref()
                        .and_then(|brand| fuzzy_score(trimmed, brand))
                })
                .or_else(|| {
                    machine
                        .load_kind
                        .and_then(|load_kind| fuzzy_score(trimmed, load_kind.as_str()))
                })
        };
        if let Some(score) = score {
            scored.push((
                score,
                MachineSuggestion {
                    uuid: machine.uuid,
                    name: machine.name,
                    machine_type: machine.machine_type,
                    brand: machine.brand,
                    model: machine.model,
                    load_kind: machine.load_kind,
                },
            ));
        }
    }
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    Ok(scored
        .into_iter()
        .take(limit)
        .map(|(_, suggestion)| suggestion)
        .collect())
}

pub fn archive_exercise(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "exercises", id, true)
}

pub fn restore_exercise(conn: &Connection, id: i64) -> Result<()> {
    set_archive(conn, "exercises", id, false)
}

pub fn start_session(conn: &Connection, gym_id: i64, notes: Option<&str>) -> Result<Session> {
    require_active_gym(conn, gym_id)?;
    let uuid = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO sessions (uuid, gym_id, notes, started_at)
         VALUES (?1, ?2, ?3, datetime('now','localtime'))",
        params![uuid, gym_id, empty_to_none(notes)],
    )
    .map_err(|err| {
        if is_unique_error(&err) {
            anyhow!("a session is already active")
        } else {
            anyhow!(err)
        }
    })?;
    get_session_by_row_id(conn, conn.last_insert_rowid())
}

pub fn current_session(conn: &Connection) -> Result<Option<Session>> {
    Ok(current_session_record(conn)?.map(|record| record.session))
}

pub fn finish_session(conn: &Connection, notes: Option<&str>) -> Result<Session> {
    let active = current_session_record(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    if let Some(notes) = notes {
        conn.execute(
            "UPDATE sessions SET notes = ?1 WHERE id = ?2",
            params![empty_to_none(Some(notes)), active.row_id],
        )?;
    }
    conn.execute(
        "UPDATE sessions SET finished_at = datetime('now','localtime') WHERE id = ?1",
        [active.row_id],
    )?;
    get_session_by_row_id(conn, active.row_id)
}

pub fn cancel_session(conn: &Connection) -> Result<()> {
    let active = current_session_record(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", [active.row_id])?;
    Ok(())
}

pub fn delete_session(conn: &Connection, session_id: &str) -> Result<Session> {
    let record = get_session_record_by_uuid(conn, session_id)?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", [record.row_id])?;
    Ok(record.session)
}

pub fn list_sessions(conn: &Connection, date: Option<&str>) -> Result<Vec<Session>> {
    if let Some(date) = date {
        validate_date(date)?;
    }
    let sql = if date.is_some() {
        "SELECT s.id, s.uuid, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         WHERE date(s.started_at) = ?1 OR date(s.started_at, 'localtime') = ?1
         ORDER BY s.started_at DESC"
    } else {
        "SELECT s.id, s.uuid, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         ORDER BY s.started_at DESC"
    };
    let mut stmt = conn.prepare(sql)?;
    let records = if let Some(date) = date {
        collect_rows(stmt.query_map([date], map_session_record)?)?
    } else {
        collect_rows(stmt.query_map([], map_session_record)?)?
    };
    Ok(records.into_iter().map(|record| record.session).collect())
}

pub fn export_session(conn: &Connection, session_id: &str) -> Result<SessionExport> {
    let record = get_session_record_by_uuid(conn, session_id)?;
    let mut stmt =
        conn.prepare("SELECT id FROM session_exercises WHERE session_id = ?1 ORDER BY position")?;
    let entry_ids = collect_rows(stmt.query_map([record.row_id], |row| row.get::<_, i64>(0))?)?;

    let mut exercises = Vec::new();
    for entry_id in entry_ids {
        let entry = get_session_exercise(conn, entry_id)?;
        let machine_id = entry.machine_id;
        let machine = machine_id
            .map(|machine_id| get_machine(conn, machine_id))
            .transpose()?;
        exercises.push(SessionExerciseExport {
            entry,
            machine,
            sets: sets_for_entry(conn, entry_id)?,
            machine_notes: machine_id
                .map(|machine_id| list_machine_notes(conn, machine_id, false))
                .transpose()?
                .unwrap_or_default(),
        });
    }

    Ok(SessionExport {
        session: record.session,
        exercises,
    })
}

pub fn import_session(conn: &mut Connection, exported: &SessionExport) -> Result<SessionExport> {
    let session_uuid = normalize_session_uuid(&exported.session.id)?;
    if session_uuid_exists(conn, &session_uuid)? {
        bail!("session {} already exists", session_uuid);
    }
    if exported.session.finished_at.is_none() && current_session(conn)?.is_some() {
        bail!("cannot import unfinished session while another session is active");
    }

    let tx = conn.transaction()?;
    let gym_id = import_gym_tx(&tx, &exported.session)?;
    tx.execute(
        "INSERT INTO sessions (uuid, gym_id, started_at, finished_at, notes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session_uuid,
            gym_id,
            exported.session.started_at,
            exported.session.finished_at,
            exported.session.notes
        ],
    )?;
    let session_row_id = tx.last_insert_rowid();
    let mut imported_machine_notes = HashSet::new();

    for exported_exercise in &exported.exercises {
        let exercise_id = import_exercise_tx(&tx, &exported_exercise.entry)?;
        let machine_id = import_machine_tx(&tx, gym_id, exported_exercise)?;
        tx.execute(
            "INSERT INTO session_exercises
                (session_id, exercise_id, machine_id, position, notes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_row_id,
                exercise_id,
                machine_id,
                exported_exercise.entry.position,
                exported_exercise.entry.notes,
                exported_exercise.entry.created_at
            ],
        )?;
        let imported_entry_id = tx.last_insert_rowid();

        for set in &exported_exercise.sets {
            if set.reps <= 0 {
                bail!("imported set reps must be greater than zero");
            }
            tx.execute(
                "INSERT INTO sets
                    (session_exercise_id, position, weight_value, weight_unit, reps, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    imported_entry_id,
                    set.position,
                    set.weight_value,
                    set.weight_unit.map(WeightUnit::as_str),
                    set.reps,
                    set.created_at
                ],
            )?;
        }

        if let Some(machine_id) = machine_id {
            for note in &exported_exercise.machine_notes {
                let key = (machine_id, note.note.trim().to_string());
                if imported_machine_notes.insert(key.clone())
                    && !machine_note_text_exists_tx(&tx, machine_id, &key.1)?
                {
                    tx.execute(
                        "INSERT INTO machine_notes
                            (machine_id, note, created_at, archived_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![machine_id, key.1, note.created_at, note.archived_at],
                    )?;
                }
            }
        }
    }

    tx.commit()?;
    export_session(conn, &exported.session.id)
}

pub fn log_exercise(
    conn: &Connection,
    exercise_id: i64,
    machine_id: Option<i64>,
    notes: Option<&str>,
    history_limit: usize,
) -> Result<LogExerciseResult> {
    let active = current_session_record(conn)?.ok_or_else(|| anyhow!("no active session"))?;
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
            if machine.gym_id != active.session.gym_id {
                bail!(
                    "machine {} does not belong to active session gym {}",
                    machine_id,
                    active.session.gym_id
                );
            }
        }
        _ if machine_id.is_some() => bail!("--machine is only valid for machine exercises"),
        _ => {}
    }

    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM session_exercises WHERE session_id = ?1",
        [active.row_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO session_exercises (session_id, exercise_id, machine_id, position, notes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now','localtime'))",
        params![
            active.row_id,
            exercise_id,
            machine_id,
            position,
            empty_to_none(notes)
        ],
    )?;
    let entry_id = conn.last_insert_rowid();
    Ok(LogExerciseResult {
        entry: get_session_exercise(conn, entry_id)?,
        history: previous_history(conn, exercise_id, active.row_id, history_limit)?,
        machine_notes: machine_id
            .map(|machine_id| {
                previous_machine_notes(conn, machine_id, history_limit.max(1), Some(entry_id))
            })
            .transpose()?
            .unwrap_or_default(),
    })
}

pub fn latest_session_exercise_id(conn: &Connection) -> Result<i64> {
    let active = current_session_record(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    conn.query_row(
        "SELECT id FROM session_exercises WHERE session_id = ?1 ORDER BY position DESC LIMIT 1",
        [active.row_id],
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

    let active = current_session_record(conn)?.ok_or_else(|| anyhow!("no active session"))?;
    let entry_session_id: i64 = conn
        .query_row(
            "SELECT session_id FROM session_exercises WHERE id = ?1",
            [session_exercise_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| anyhow!("session exercise {} not found", session_exercise_id))?;
    if entry_session_id != active.row_id {
        bail!("sets can only be added to the active session");
    }

    let position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), 0) + 1 FROM sets WHERE session_exercise_id = ?1",
        [session_exercise_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO sets (session_exercise_id, position, weight_value, weight_unit, reps, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now','localtime'))",
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

pub fn exercise_history(
    conn: &Connection,
    exercise_id: i64,
    limit: usize,
) -> Result<Vec<HistoryEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT se.id, s.uuid, s.started_at, g.name, m.name
         FROM session_exercises se
         JOIN sessions s ON s.id = se.session_id
         JOIN gyms g ON g.id = s.gym_id
         LEFT JOIN machines m ON m.id = se.machine_id
         WHERE se.exercise_id = ?1
           AND s.finished_at IS NOT NULL
         ORDER BY s.started_at DESC, se.position DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![exercise_id, limit as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    history_entries_from_rows(conn, rows)
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

fn session_uuid_exists(conn: &Connection, uuid: &str) -> Result<bool> {
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM sessions WHERE uuid = ?1", [uuid], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(exists.is_some())
}

fn import_gym_tx(tx: &rusqlite::Transaction<'_>, session: &Session) -> Result<i64> {
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM gyms WHERE lower(name) = lower(?1) AND archived_at IS NULL ORDER BY id LIMIT 1",
            [session.gym_name.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(id);
    }

    require_text(&session.gym_name, "gym name")?;
    let slug = unique_slug(tx, "gyms", &session.gym_name)?;
    tx.execute(
        "INSERT INTO gyms (name, slug) VALUES (?1, ?2)",
        params![session.gym_name.trim(), slug],
    )?;
    Ok(tx.last_insert_rowid())
}

fn import_exercise_tx(tx: &rusqlite::Transaction<'_>, entry: &SessionExercise) -> Result<i64> {
    if let Some(id) = tx
        .query_row(
            "SELECT id FROM exercises
             WHERE archived_at IS NULL AND lower(name) = lower(?1)
             ORDER BY id LIMIT 1",
            [entry.exercise_name.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(id);
    }

    require_text(&entry.exercise_name, "exercise name")?;
    let slug = unique_slug(tx, "exercises", &entry.exercise_name)?;
    tx.execute(
        "INSERT INTO exercises (name, slug, kind) VALUES (?1, ?2, ?3)",
        params![entry.exercise_name.trim(), slug, entry.kind.as_str()],
    )?;
    Ok(tx.last_insert_rowid())
}

fn import_machine_tx(
    tx: &rusqlite::Transaction<'_>,
    gym_id: i64,
    exported: &SessionExerciseExport,
) -> Result<Option<i64>> {
    if exported.entry.kind != ExerciseKind::Machine {
        return Ok(None);
    }

    let machine = exported.machine.as_ref().ok_or_else(|| {
        anyhow!(
            "machine exercise '{}' has no exported machine",
            exported.entry.exercise_name
        )
    })?;
    let machine_uuid = normalize_machine_uuid(&machine.uuid)?;
    if let Some((id, existing_gym_id)) = tx
        .query_row(
            "SELECT id, gym_id FROM machines WHERE uuid = ?1",
            [machine_uuid.as_str()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        if existing_gym_id != gym_id {
            bail!(
                "machine {} already exists under a different gym",
                machine_uuid
            );
        }
        return Ok(Some(id));
    }

    require_text(&machine.name, "machine name")?;
    require_text(&machine.machine_type, "machine type")?;
    tx.execute(
        "INSERT INTO machines
            (uuid, gym_id, name, machine_type, brand, model, load_kind, settings_notes, created_at, archived_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            machine_uuid,
            gym_id,
            machine.name.trim(),
            machine.machine_type.trim(),
            normalize_machine_brand(machine.brand.as_deref())?,
            machine.model,
            machine.load_kind.map(MachineLoadKind::as_str),
            machine.settings_notes,
            machine.created_at,
            machine.archived_at
        ],
    )?;
    Ok(Some(tx.last_insert_rowid()))
}

fn machine_note_text_exists_tx(
    tx: &rusqlite::Transaction<'_>,
    machine_id: i64,
    note: &str,
) -> Result<bool> {
    let exists: Option<i64> = tx
        .query_row(
            "SELECT id FROM machine_notes
             WHERE machine_id = ?1 AND note = ?2 AND archived_at IS NULL
             LIMIT 1",
            params![machine_id, note],
            |row| row.get(0),
        )
        .optional()?;
    Ok(exists.is_some())
}

fn get_machine(conn: &Connection, id: i64) -> Result<Machine> {
    conn.query_row(
        "SELECT id, uuid, gym_id, name, machine_type, brand, model, load_kind, settings_notes, created_at, archived_at
         FROM machines WHERE id = ?1",
        [id],
        map_machine,
    )
    .optional()?
    .ok_or_else(|| anyhow!("machine {} not found", id))
}

fn get_machine_note(conn: &Connection, id: i64) -> Result<MachineNote> {
    conn.query_row(
        "SELECT id, machine_id, session_exercise_id, note, created_at, archived_at
         FROM machine_notes WHERE id = ?1",
        [id],
        map_machine_note,
    )
    .optional()?
    .ok_or_else(|| anyhow!("machine note {} not found", id))
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

fn get_session_by_row_id(conn: &Connection, id: i64) -> Result<Session> {
    Ok(get_session_record_by_row_id(conn, id)?.session)
}

fn get_session_record_by_row_id(conn: &Connection, id: i64) -> Result<SessionRecord> {
    conn.query_row(
        "SELECT s.id, s.uuid, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         WHERE s.id = ?1",
        [id],
        map_session_record,
    )
    .optional()?
    .ok_or_else(|| anyhow!("session {} not found", id))
}

fn get_session_record_by_uuid(conn: &Connection, uuid: &str) -> Result<SessionRecord> {
    let uuid = normalize_session_uuid(uuid)?;
    conn.query_row(
        "SELECT s.id, s.uuid, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         WHERE s.uuid = ?1",
        [uuid.as_str()],
        map_session_record,
    )
    .optional()?
    .ok_or_else(|| anyhow!("session {} not found", uuid))
}

fn current_session_record(conn: &Connection) -> Result<Option<SessionRecord>> {
    conn.query_row(
        "SELECT s.id, s.uuid, s.gym_id, g.name, s.started_at, s.finished_at, s.notes
         FROM sessions s JOIN gyms g ON g.id = s.gym_id
         WHERE s.finished_at IS NULL",
        [],
        map_session_record,
    )
    .optional()
    .map_err(Into::into)
}

fn get_session_exercise(conn: &Connection, id: i64) -> Result<SessionExercise> {
    conn.query_row(
        "SELECT se.id, s.uuid, se.exercise_id, e.name, e.kind, se.machine_id, m.name, se.position, se.notes, se.created_at
         FROM session_exercises se
         JOIN sessions s ON s.id = se.session_id
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
        "SELECT se.id, s.uuid, s.started_at, g.name, m.name
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
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    history_entries_from_rows(conn, rows)
}

fn history_entries_from_rows(
    conn: &Connection,
    rows: rusqlite::MappedRows<
        '_,
        impl FnMut(&Row<'_>) -> rusqlite::Result<(i64, String, String, String, Option<String>)>,
    >,
) -> Result<Vec<HistoryEntry>> {
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

fn previous_machine_notes(
    conn: &Connection,
    machine_id: i64,
    limit: usize,
    exclude_session_exercise_id: Option<i64>,
) -> Result<Vec<MachineNote>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT id, machine_id, session_exercise_id, note, created_at, archived_at
         FROM machine_notes
         WHERE machine_id = ?1
           AND archived_at IS NULL
           AND (?2 IS NULL OR session_exercise_id IS NULL OR session_exercise_id != ?2)
         ORDER BY created_at DESC, id DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![machine_id, exclude_session_exercise_id, limit as i64],
        map_machine_note,
    )?;
    collect_rows(rows)
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
    for tag in normalized_tags(tags)? {
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

fn seed_allowed_tags(conn: &Connection) -> Result<()> {
    for tag in ALLOWED_TAGS {
        conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [tag])?;
    }
    Ok(())
}

fn normalized_tags(tags: &[String]) -> Result<Vec<String>> {
    let mut normalized = tags
        .iter()
        .map(|tag| normalize_allowed_tag(tag))
        .collect::<Result<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn normalize_allowed_tag(tag: &str) -> Result<String> {
    let normalized = tag.trim().to_lowercase();
    if normalized.is_empty() {
        bail!("tag cannot be empty");
    }
    if ALLOWED_TAGS.contains(&normalized.as_str()) {
        Ok(normalized)
    } else {
        bail!(
            "unknown tag '{}'; allowed tags: {}",
            tag,
            ALLOWED_TAGS.join(", ")
        );
    }
}

fn normalize_machine_brand(brand: Option<&str>) -> Result<Option<String>> {
    let Some(brand) = empty_to_none(brand) else {
        return Ok(None);
    };
    require_text(&brand, "machine brand")?;
    let normalized = normalize_search(&brand);
    if let Some(preset) = PRESET_MACHINE_BRANDS
        .iter()
        .find(|preset| normalize_search(preset) == normalized)
    {
        return Ok(Some((*preset).to_string()));
    }
    Ok(MACHINE_BRAND_ALIASES
        .iter()
        .find(|(alias, _)| normalize_search(alias) == normalized)
        .map(|(_, preset)| (*preset).to_string())
        .or(Some(brand)))
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
        format!("UPDATE {table} SET archived_at = datetime('now','localtime') WHERE id = ?1")
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

fn normalize_search(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn normalize_session_uuid(value: &str) -> Result<String> {
    let trimmed = value.trim();
    let uuid = Uuid::parse_str(trimmed).map_err(|_| anyhow!("session id must be a UUID"))?;
    Ok(uuid.to_string())
}

fn normalize_machine_uuid(value: &str) -> Result<String> {
    let trimmed = value.trim();
    let uuid = Uuid::parse_str(trimmed).map_err(|_| anyhow!("machine must be a UUID"))?;
    Ok(uuid.to_string())
}

fn validate_date(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let valid = bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    if !valid {
        bail!("date must be in YYYY-MM-DD format");
    }
    Ok(())
}

fn fuzzy_score(query: &str, candidate: &str) -> Option<usize> {
    let query = normalize_search(query);
    let candidate = normalize_search(candidate);
    if query.is_empty() || candidate.is_empty() || query == candidate {
        return None;
    }
    if candidate.contains(&query) {
        return Some(10_000 + query.len());
    }
    if query.contains(&candidate) {
        return Some(9_000 + candidate.len());
    }

    let distance = levenshtein(&query, &candidate);
    let max_len = query.len().max(candidate.len());
    if distance <= 2 || distance * 3 <= max_len {
        Some(1_000usize.saturating_sub(distance))
    } else {
        None
    }
}

fn levenshtein(left: &str, right: &str) -> usize {
    let mut costs = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.chars().enumerate() {
        let mut previous = costs[0];
        costs[0] = left_index + 1;
        for (right_index, right_char) in right.chars().enumerate() {
            let current = costs[right_index + 1];
            costs[right_index + 1] = if left_char == right_char {
                previous
            } else {
                1 + previous.min(current).min(costs[right_index])
            };
            previous = current;
        }
    }
    costs[right.len()]
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
        uuid: row.get(1)?,
        gym_id: row.get(2)?,
        name: row.get(3)?,
        machine_type: row.get(4)?,
        brand: row.get(5)?,
        model: row.get(6)?,
        load_kind: row
            .get::<_, Option<String>>(7)?
            .map(|value| MachineLoadKind::from_str(&value))
            .transpose()
            .map_err(to_sql_error)?,
        settings_notes: row.get(8)?,
        created_at: row.get(9)?,
        archived_at: row.get(10)?,
    })
}

fn map_machine_note(row: &Row<'_>) -> rusqlite::Result<MachineNote> {
    Ok(MachineNote {
        id: row.get(0)?,
        machine_id: row.get(1)?,
        session_exercise_id: row.get(2)?,
        note: row.get(3)?,
        created_at: row.get(4)?,
        archived_at: row.get(5)?,
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

fn map_session_record(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        row_id: row.get(0)?,
        session: Session {
            id: row.get(1)?,
            gym_id: row.get(2)?,
            gym_name: row.get(3)?,
            started_at: row.get(4)?,
            finished_at: row.get(5)?,
            notes: row.get(6)?,
        },
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
    fn schema_adds_machine_uuids_to_existing_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE gyms (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                address TEXT,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
                archived_at TEXT
            );
            CREATE TABLE machines (
                id INTEGER PRIMARY KEY,
                gym_id INTEGER NOT NULL REFERENCES gyms(id),
                name TEXT NOT NULL,
                machine_type TEXT NOT NULL,
                brand TEXT,
                model TEXT,
                settings_notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
                archived_at TEXT
            );
            INSERT INTO gyms (id, name, slug) VALUES (1, 'Main', 'main');
            INSERT INTO machines (id, gym_id, name, machine_type) VALUES (1, 1, 'Leg Press', 'leg-press');
            "#,
        )
        .unwrap();

        ensure_schema(&conn).unwrap();

        let machine = list_machines(&conn, 1, false).unwrap().remove(0);
        Uuid::parse_str(&machine.uuid).unwrap();
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
            &["triceps".into(), "chest".into()],
            None,
        )
        .unwrap();

        let chest = list_exercises(&mutable, Some("chest"), None, false).unwrap();
        assert_eq!(chest.len(), 1);
        assert_eq!(chest[0].tags, vec!["chest", "triceps"]);
    }

    #[test]
    fn tags_are_restricted_to_allowlist() {
        let conn = conn();
        let mut mutable = conn;
        let err = add_exercise(
            &mut mutable,
            "Bench Press",
            ExerciseKind::Freeweight,
            &["push".into()],
            None,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("unknown tag"));
        assert!(err.contains("biceps"));
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
    fn delete_session_removes_finished_session_and_children() {
        let conn = conn();
        let mut mutable = conn;
        let gym = add_gym(&mutable, "Main", None, None).unwrap();
        let exercise =
            add_exercise(&mut mutable, "Squat", ExerciseKind::Freeweight, &[], None).unwrap();

        let session = start_session(&mutable, gym.id, None).unwrap();
        let session_row_id: i64 = mutable
            .query_row(
                "SELECT id FROM sessions WHERE uuid = ?1",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        let entry = log_exercise(&mutable, exercise.id, None, None, 1).unwrap();
        log_set(
            &mutable,
            entry.entry.id,
            5,
            Some(100.0),
            Some(WeightUnit::Kg),
        )
        .unwrap();
        finish_session(&mutable, None).unwrap();

        let deleted = delete_session(&mutable, &session.id).unwrap();
        assert_eq!(deleted.id, session.id);
        let remaining: i64 = mutable
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE uuid = ?1",
                [&session.id],
                |row| row.get(0),
            )
            .unwrap();
        let remaining_entries: i64 = mutable
            .query_row(
                "SELECT COUNT(*) FROM session_exercises WHERE session_id = ?1",
                [session_row_id],
                |row| row.get(0),
            )
            .unwrap();
        let remaining_sets: i64 = mutable
            .query_row("SELECT COUNT(*) FROM sets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(remaining_entries, 0);
        assert_eq!(remaining_sets, 0);
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

    #[test]
    fn fuzzy_suggestions_do_not_resolve_exercises() {
        let conn = conn();
        let mut mutable = conn;
        add_exercise(
            &mut mutable,
            "Bench Press",
            ExerciseKind::Freeweight,
            &["chest".into()],
            None,
        )
        .unwrap();

        assert!(find_exercise_exact(&mutable, "bench").unwrap().is_none());
        let suggestions = suggest_exercises(&mutable, "bench", 5).unwrap();
        assert_eq!(suggestions[0].name, "Bench Press");
    }

    #[test]
    fn machine_notes_are_returned_when_logging_machine_exercise() {
        let conn = conn();
        let mut mutable = conn;
        let gym = add_gym(&mutable, "Main", None, None).unwrap();
        let machine = add_machine(
            &mutable,
            gym.id,
            "Leg Press A",
            "leg-press",
            None,
            None,
            None,
            None,
        )
        .unwrap();
        add_machine_note(&mutable, machine.id, "seat 4").unwrap();
        let exercise = add_exercise(
            &mut mutable,
            "Leg Press",
            ExerciseKind::Machine,
            &["quad".into()],
            None,
        )
        .unwrap();
        start_session(&mutable, gym.id, None).unwrap();

        let logged = log_exercise(&mutable, exercise.id, Some(machine.id), None, 1).unwrap();

        assert_eq!(logged.machine_notes.len(), 1);
        assert_eq!(logged.machine_notes[0].note, "seat 4");
        let all_notes = list_machine_notes(&mutable, machine.id, false).unwrap();
        assert_eq!(all_notes.len(), 1);
    }
}
