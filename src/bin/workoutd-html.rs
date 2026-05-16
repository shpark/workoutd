use anyhow::{bail, Context, Result};
use clap::Parser;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use workoutd::{
    default_db_path, exercise_history, export_session, find_exercise_exact, open_database,
    HistoryEntry, SessionExport, SetEntry, WeightUnit,
};

#[derive(Parser)]
#[command(name = "workoutd-html")]
#[command(about = "Render a workoutd session export as a standalone HTML report")]
struct Cli {
    /// Database path. Used when SESSION_UUID is provided instead of --input.
    #[arg(long, env = "WORKOUTD_DB")]
    db: Option<PathBuf>,
    /// Read JSON created by `workoutd session export`.
    #[arg(long)]
    input: Option<PathBuf>,
    /// Write HTML to this path instead of stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Timezone label for displayed timestamps. Defaults to TZ or "local".
    #[arg(long, env = "TZ")]
    timezone: Option<String>,
    /// Render completed session history for this exercise instead of a single session.
    #[arg(long)]
    exercise_history: Option<String>,
    /// Maximum history entries for --exercise-history.
    #[arg(long, default_value_t = 12)]
    history_limit: usize,
    /// Session UUID to export from the database.
    session_id: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let timezone = cli.timezone.clone().unwrap_or_else(default_timezone_label);
    let html = if let Some(exercise_query) = cli.exercise_history.as_deref() {
        if cli.input.is_some() || cli.session_id.is_some() {
            bail!("--exercise-history cannot be combined with --input or SESSION_UUID");
        }
        let (exercise_name, entries) = load_exercise_history(&cli, exercise_query)?;
        render_history_html(&exercise_name, &entries, &timezone)?
    } else {
        let exported = load_session(&cli)?;
        render_html(&exported, &timezone)?
    };

    if let Some(path) = cli.output {
        fs::write(&path, html).with_context(|| format!("write {}", path.display()))?;
    } else {
        io::stdout().write_all(html.as_bytes())?;
    }

    Ok(())
}

fn load_exercise_history(cli: &Cli, query: &str) -> Result<(String, Vec<HistoryEntry>)> {
    let db_path = cli.db.clone().map(Ok).unwrap_or_else(default_db_path)?;
    let conn = open_database(&db_path)?;
    let exercise = find_exercise_exact(&conn, query)?
        .ok_or_else(|| anyhow::anyhow!("exercise '{}' not found", query))?;
    let entries = exercise_history(&conn, exercise.id, cli.history_limit)?;
    Ok((exercise.name, entries))
}

fn load_session(cli: &Cli) -> Result<SessionExport> {
    match (&cli.input, &cli.session_id) {
        (Some(_), Some(_)) => bail!("provide either --input or SESSION_UUID, not both"),
        (Some(path), None) => {
            let input =
                fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            serde_json::from_str(&input).with_context(|| format!("parse {}", path.display()))
        }
        (None, Some(session_id)) => {
            let db_path = cli.db.clone().map(Ok).unwrap_or_else(default_db_path)?;
            let conn = open_database(&db_path)?;
            export_session(&conn, session_id)
        }
        (None, None) => bail!("provide --input PATH or SESSION_UUID"),
    }
}

fn render_html(exported: &SessionExport, timezone: &str) -> Result<String> {
    let session = &exported.session;
    let title = format!(
        "Workout {} {} - {}",
        session.started_at, timezone, session.gym_name
    );
    let finished = session.finished_at.as_deref().unwrap_or("unfinished");
    let total_sets: usize = exported
        .exercises
        .iter()
        .map(|exercise| exercise.sets.len())
        .sum();

    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("  <meta charset=\"utf-8\">\n");
    html.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("  <title>{}</title>\n", escape_html(&title)));
    html.push_str(STYLES);
    html.push_str("</head>\n<body>\n");
    html.push_str("  <main class=\"page\">\n");
    html.push_str("    <header class=\"session-header\">\n");
    html.push_str(&format!(
        "      <p class=\"eyebrow\">{}</p>\n",
        escape_html(&session.gym_name)
    ));
    html.push_str(&format!(
        "      <h1>{}</h1>\n",
        escape_html(&format!("{} {}", session.started_at, timezone))
    ));
    html.push_str("      <dl class=\"summary-grid\">\n");
    html.push_str(&summary_item("Session", &session.id));
    html.push_str(&summary_item("Timezone", timezone));
    html.push_str(&summary_item("Finished", finished));
    html.push_str(&summary_item(
        "Exercises",
        &exported.exercises.len().to_string(),
    ));
    html.push_str(&summary_item("Sets", &total_sets.to_string()));
    html.push_str("      </dl>\n");
    if let Some(notes) = session.notes.as_deref() {
        html.push_str(&format!(
            "      <p class=\"session-notes\">{}</p>\n",
            escape_html(notes)
        ));
    }
    html.push_str("    </header>\n");

    for exercise in &exported.exercises {
        html.push_str("    <section class=\"exercise\">\n");
        html.push_str("      <div class=\"exercise-title-row\">\n");
        html.push_str(&format!(
            "        <h2>{}. {}</h2>\n",
            exercise.entry.position,
            escape_html(&exercise.entry.exercise_name)
        ));
        html.push_str(&format!(
            "        <span class=\"kind\">{}</span>\n",
            escape_html(exercise.entry.kind.as_str())
        ));
        html.push_str("      </div>\n");

        if let Some(machine) = exercise.machine.as_ref() {
            let mut parts = vec![machine.machine_type.as_str()];
            if let Some(brand) = machine.brand.as_deref() {
                parts.push(brand);
            }
            if let Some(model) = machine.model.as_deref() {
                parts.push(model);
            }
            if let Some(load_kind) = machine.load_kind {
                parts.push(load_kind.as_str());
            }
            html.push_str(&format!(
                "      <p class=\"machine\"><strong>{}</strong> <span>{}</span></p>\n",
                escape_html(&machine.name),
                escape_html(&parts.join(" / "))
            ));
        }

        if let Some(notes) = exercise.entry.notes.as_deref() {
            html.push_str(&format!(
                "      <p class=\"entry-notes\">{}</p>\n",
                escape_html(notes)
            ));
        }

        if !exercise.machine_notes.is_empty() {
            html.push_str("      <ul class=\"machine-notes\">\n");
            for note in &exercise.machine_notes {
                html.push_str(&format!("        <li>{}</li>\n", escape_html(&note.note)));
            }
            html.push_str("      </ul>\n");
        }

        html.push_str("      <table>\n");
        html.push_str("        <thead><tr><th>Set</th><th>Load</th><th>Reps</th><th>Recorded</th></tr></thead>\n");
        html.push_str("        <tbody>\n");
        if exercise.sets.is_empty() {
            html.push_str(
                "          <tr><td colspan=\"4\" class=\"empty\">No sets recorded</td></tr>\n",
            );
        } else {
            for set in &exercise.sets {
                html.push_str(&format!(
                    "          <tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                    set.position,
                    escape_html(&load_label(set)?),
                    set.reps,
                    escape_html(&set.created_at)
                ));
            }
        }
        html.push_str("        </tbody>\n");
        html.push_str("      </table>\n");
        html.push_str("    </section>\n");
    }

    html.push_str("  </main>\n</body>\n</html>\n");
    Ok(html)
}

fn render_history_html(
    exercise_name: &str,
    entries: &[HistoryEntry],
    timezone: &str,
) -> Result<String> {
    let mut chronological = entries.iter().collect::<Vec<_>>();
    chronological.reverse();
    let metrics = chronological
        .iter()
        .map(|entry| history_metric(entry))
        .collect::<Vec<_>>();
    let weighted_unit = uniform_weight_unit(&metrics);
    let has_weighted_metrics = weighted_unit.is_some()
        && metrics
            .iter()
            .any(|metric| metric.total_volume.is_some() || metric.max_weight.is_some());
    let title = format!("{} history", exercise_name);
    let total_sets: usize = entries.iter().map(|entry| entry.sets.len()).sum();
    let total_reps: i64 = entries
        .iter()
        .flat_map(|entry| entry.sets.iter())
        .map(|set| set.reps)
        .sum();
    let total_volume = metrics
        .iter()
        .filter_map(|metric| metric.total_volume)
        .sum::<f64>();
    let max_weight = max_f64(metrics.iter().filter_map(|metric| metric.max_weight));

    let mut html = String::new();
    html.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("  <meta charset=\"utf-8\">\n");
    html.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str(&format!("  <title>{}</title>\n", escape_html(&title)));
    html.push_str(STYLES);
    html.push_str("</head>\n<body>\n");
    html.push_str("  <main class=\"page\">\n");
    html.push_str("    <header class=\"session-header\">\n");
    html.push_str("      <p class=\"eyebrow\">Exercise history</p>\n");
    html.push_str(&format!("      <h1>{}</h1>\n", escape_html(exercise_name)));
    html.push_str("      <dl class=\"summary-grid\">\n");
    html.push_str(&summary_item("Timezone", timezone));
    html.push_str(&summary_item("Sessions", &entries.len().to_string()));
    html.push_str(&summary_item("Sets", &total_sets.to_string()));
    if let Some(unit) = weighted_unit.filter(|_| has_weighted_metrics) {
        html.push_str(&summary_item(
            "Total Volume",
            &format_metric(total_volume, volume_unit_label(unit)),
        ));
        if let Some(weight) = max_weight {
            html.push_str(&summary_item(
                "Max Weight",
                &format_metric(weight, unit.as_str()),
            ));
        }
    } else {
        html.push_str(&summary_item("Total Reps", &total_reps.to_string()));
    }
    html.push_str("      </dl>\n");
    html.push_str("    </header>\n");

    if let Some(unit) = weighted_unit.filter(|_| has_weighted_metrics) {
        html.push_str(&history_chart_section(
            "Volume and max weight by session",
            &dual_metric_chart_svg(&metrics, unit),
        ));
    } else {
        html.push_str(&history_chart_section(
            "Total reps by session",
            &metric_chart_svg(&rep_points(&metrics), "Total reps by session"),
        ));
    }

    html.push_str("    <section class=\"exercise\">\n");
    html.push_str("      <div class=\"exercise-title-row\">\n");
    html.push_str("        <h2>Session details</h2>\n");
    html.push_str("      </div>\n");
    html.push_str("      <table>\n");
    html.push_str("        <thead><tr><th>Date</th><th>Gym</th><th>Machine</th><th>Set</th><th>Load</th><th>Reps</th><th>Session Volume</th><th>Session Max</th></tr></thead>\n");
    if metrics.is_empty() {
        html.push_str("        <tbody>\n");
        html.push_str(
            "          <tr><td colspan=\"8\" class=\"empty\">No completed history</td></tr>\n",
        );
        html.push_str("        </tbody>\n");
    } else {
        for metric in &metrics {
            let entry = metric.entry;
            let row_count = entry.sets.len().max(1);
            let max_weight = metric
                .max_weight
                .zip(metric.unit)
                .map(|(weight, unit)| format_metric(weight, unit.as_str()))
                .unwrap_or_else(|| "-".to_string());
            let volume = metric
                .total_volume
                .zip(metric.unit)
                .map(|(volume, unit)| format_metric(volume, volume_unit_label(unit)))
                .unwrap_or_else(|| "-".to_string());
            let date = format!("{} {}", entry.session_date, timezone);
            let machine = entry.machine_name.as_deref().unwrap_or("-");
            html.push_str("        <tbody class=\"session-group\">\n");
            if entry.sets.is_empty() {
                html.push_str(&format!(
                    "          <tr><td class=\"session-cell\">{}</td><td class=\"session-cell\">{}</td><td class=\"session-cell\">{}</td><td>-</td><td>-</td><td>-</td><td class=\"session-cell\">{}</td><td class=\"session-cell\">{}</td></tr>\n",
                    escape_html(&date),
                    escape_html(&entry.gym_name),
                    escape_html(machine),
                    escape_html(&volume),
                    escape_html(&max_weight)
                ));
            } else {
                for (index, set) in entry.sets.iter().enumerate() {
                    if index == 0 {
                        html.push_str(&format!(
                            "          <tr><td rowspan=\"{}\" class=\"session-cell\">{}</td><td rowspan=\"{}\" class=\"session-cell\">{}</td><td rowspan=\"{}\" class=\"session-cell\">{}</td><td>{}</td><td>{}</td><td>{}</td><td rowspan=\"{}\" class=\"session-cell\">{}</td><td rowspan=\"{}\" class=\"session-cell\">{}</td></tr>\n",
                            row_count,
                            escape_html(&date),
                            row_count,
                            escape_html(&entry.gym_name),
                            row_count,
                            escape_html(machine),
                            set.position,
                            escape_html(&load_label(set)?),
                            set.reps,
                            row_count,
                            escape_html(&volume),
                            row_count,
                            escape_html(&max_weight)
                        ));
                    } else {
                        html.push_str(&format!(
                            "          <tr><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                            set.position,
                            escape_html(&load_label(set)?),
                            set.reps
                        ));
                    }
                }
            }
            html.push_str("        </tbody>\n");
        }
    }
    html.push_str("      </table>\n");
    html.push_str("    </section>\n");
    html.push_str("  </main>\n</body>\n</html>\n");
    Ok(html)
}

struct HistoryMetric<'a> {
    entry: &'a HistoryEntry,
    total_reps: i64,
    total_volume: Option<f64>,
    max_weight: Option<f64>,
    unit: Option<WeightUnit>,
}

struct ChartPoint {
    date: String,
    value: f64,
    label: String,
}

fn history_metric(entry: &HistoryEntry) -> HistoryMetric<'_> {
    let total_reps = entry.sets.iter().map(|set| set.reps).sum();
    let mut unit = None;
    let mut mixed_units = false;
    let mut total_volume = 0.0;
    let mut max_weight = None;

    for set in &entry.sets {
        if let (Some(weight), Some(set_unit)) = (set.weight_value, set.weight_unit) {
            match unit {
                Some(unit) if unit != set_unit => mixed_units = true,
                None => unit = Some(set_unit),
                _ => {}
            }
            total_volume += weight * set.reps as f64;
            max_weight = Some(max_weight.map_or(weight, |current: f64| current.max(weight)));
        }
    }

    HistoryMetric {
        entry,
        total_reps,
        total_volume: unit.filter(|_| !mixed_units).map(|_| total_volume),
        max_weight: unit.filter(|_| !mixed_units).and(max_weight),
        unit: unit.filter(|_| !mixed_units),
    }
}

fn uniform_weight_unit(metrics: &[HistoryMetric<'_>]) -> Option<WeightUnit> {
    let mut unit = None;
    for metric_unit in metrics.iter().filter_map(|metric| metric.unit) {
        match unit {
            Some(unit) if unit != metric_unit => return None,
            None => unit = Some(metric_unit),
            _ => {}
        }
    }
    unit
}

fn rep_points(metrics: &[HistoryMetric<'_>]) -> Vec<ChartPoint> {
    metrics
        .iter()
        .map(|metric| ChartPoint {
            date: metric.entry.session_date.clone(),
            value: metric.total_reps as f64,
            label: format!("{} reps", metric.total_reps),
        })
        .collect()
}

fn history_chart_section(title: &str, chart: &str) -> String {
    format!(
        "    <section class=\"exercise\">\n      <div class=\"exercise-title-row\">\n        <h2>{}</h2>\n        <span class=\"kind\">completed sessions</span>\n      </div>\n{}    </section>\n",
        escape_html(title),
        chart
    )
}

fn dual_metric_chart_svg(metrics: &[HistoryMetric<'_>], unit: WeightUnit) -> String {
    const WIDTH: f64 = 840.0;
    const HEIGHT: f64 = 320.0;
    const TOP: f64 = 54.0;
    const RIGHT: f64 = 82.0;
    const BOTTOM: f64 = 54.0;
    const LEFT: f64 = 82.0;
    const X_INSET: f64 = 58.0;

    if metrics.is_empty() {
        return "      <div class=\"chart-empty\">No completed history</div>\n".to_string();
    }

    let volumes = metrics
        .iter()
        .filter_map(|metric| metric.total_volume)
        .collect::<Vec<_>>();
    let weights = metrics
        .iter()
        .filter_map(|metric| metric.max_weight)
        .collect::<Vec<_>>();
    let (min_volume, max_volume) = padded_domain(&volumes);
    let (min_weight, max_weight) = padded_domain(&weights);
    let plot_width = WIDTH - LEFT - RIGHT;
    let plot_height = HEIGHT - TOP - BOTTOM;
    let sample_width = (plot_width - X_INSET * 2.0).max(1.0);
    let step = if metrics.len() > 1 {
        sample_width / (metrics.len() - 1) as f64
    } else {
        0.0
    };

    let mut svg = String::new();
    svg.push_str("      <svg class=\"history-chart\" viewBox=\"0 0 840 320\" role=\"img\" aria-label=\"Volume and max weight by session\">\n");
    svg.push_str(&format!(
        "        <line x1=\"{LEFT}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"axis\" />\n",
        HEIGHT - BOTTOM,
        WIDTH - RIGHT,
        HEIGHT - BOTTOM
    ));
    svg.push_str(&format!(
        "        <line x1=\"{LEFT}\" y1=\"{TOP}\" x2=\"{LEFT}\" y2=\"{}\" class=\"axis\" />\n",
        HEIGHT - BOTTOM
    ));
    svg.push_str(&format!(
        "        <line x1=\"{}\" y1=\"{TOP}\" x2=\"{}\" y2=\"{}\" class=\"axis axis-right\" />\n",
        WIDTH - RIGHT,
        WIDTH - RIGHT,
        HEIGHT - BOTTOM
    ));
    svg.push_str(&axis_tick(
        LEFT,
        HEIGHT - BOTTOM,
        &format_number(min_volume.max(0.0)),
        "volume-tick",
        false,
    ));
    svg.push_str(&axis_tick(
        LEFT,
        TOP,
        &format_number(max_volume),
        "volume-tick",
        false,
    ));
    svg.push_str(&axis_tick(
        WIDTH - RIGHT,
        HEIGHT - BOTTOM,
        &format_number(min_weight.max(0.0)),
        "weight-tick",
        true,
    ));
    svg.push_str(&axis_tick(
        WIDTH - RIGHT,
        TOP,
        &format_number(max_weight),
        "weight-tick",
        true,
    ));
    svg.push_str(&format!(
        "        <text x=\"{LEFT}\" y=\"24\" class=\"axis-label\">Volume ({})</text>\n",
        escape_html(volume_unit_label(unit))
    ));
    svg.push_str(&format!(
        "        <text x=\"{}\" y=\"24\" class=\"axis-label axis-label-right\">Max weight ({})</text>\n",
        WIDTH - RIGHT,
        escape_html(unit.as_str())
    ));
    svg.push_str("        <g class=\"chart-legend\" aria-hidden=\"true\">\n");
    svg.push_str(
        "          <line x1=\"304\" y1=\"18\" x2=\"328\" y2=\"18\" class=\"volume-line\" />\n",
    );
    svg.push_str("          <circle cx=\"316\" cy=\"18\" r=\"4\" class=\"volume-dot\" />\n");
    svg.push_str("          <text x=\"332\" y=\"23\">volume</text>\n");
    svg.push_str(
        "          <line x1=\"400\" y1=\"18\" x2=\"424\" y2=\"18\" class=\"weight-line\" />\n",
    );
    svg.push_str("          <circle cx=\"412\" cy=\"18\" r=\"4\" class=\"weight-dot\" />\n");
    svg.push_str("          <text x=\"432\" y=\"23\">max weight</text>\n");
    svg.push_str("        </g>\n");

    let mut volume_points = Vec::new();
    let mut weight_points = Vec::new();
    for (index, metric) in metrics.iter().enumerate() {
        let x = if metrics.len() == 1 {
            LEFT + plot_width / 2.0
        } else {
            LEFT + X_INSET + index as f64 * step
        };
        svg.push_str(&format!(
            "        <text x=\"{x:.1}\" y=\"{}\" class=\"x-label\">{}</text>\n",
            HEIGHT - 28.0,
            escape_html(&metric.entry.session_date[..metric.entry.session_date.len().min(10)])
        ));

        if let Some(volume) = metric.total_volume {
            let volume_y = scale_y(volume, min_volume, max_volume, plot_height, HEIGHT, BOTTOM);
            let volume_label = format_metric(volume, volume_unit_label(unit));
            volume_points.push((
                x,
                volume_y,
                metric.entry.session_date.as_str(),
                volume_label,
            ));
        }
        if let Some(weight) = metric.max_weight {
            let weight_y = scale_y(weight, min_weight, max_weight, plot_height, HEIGHT, BOTTOM);
            let weight_label = format_metric(weight, unit.as_str());
            weight_points.push((
                x,
                weight_y,
                metric.entry.session_date.as_str(),
                weight_label,
            ));
        }
    }

    svg.push_str(&line_path(&volume_points, "volume-line"));
    svg.push_str(&line_path(&weight_points, "weight-line"));
    for (x, y, date, label) in volume_points {
        svg.push_str(&format!(
            "        <circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"5\" class=\"volume-dot\"><title>{}: volume {}</title></circle>\n",
            escape_html(date),
            escape_html(&label)
        ));
        let label_y = if y < TOP + 18.0 { y + 20.0 } else { y - 10.0 };
        svg.push_str(&format!(
            "        <text x=\"{x:.1}\" y=\"{label_y:.1}\" class=\"volume-tag\">{}</text>\n",
            escape_html(&label)
        ));
    }
    for (x, y, date, label) in weight_points {
        svg.push_str(&format!(
            "        <circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"5\" class=\"weight-dot\"><title>{}: max weight {}</title></circle>\n",
            escape_html(date),
            escape_html(&label)
        ));
        let label_y = if y < TOP + 18.0 { y + 20.0 } else { y - 10.0 };
        svg.push_str(&format!(
            "        <text x=\"{x:.1}\" y=\"{label_y:.1}\" class=\"weight-tag\">{}</text>\n",
            escape_html(&label)
        ));
    }

    svg.push_str("      </svg>\n");
    svg
}

fn padded_domain(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 1.0);
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() < 0.001 {
        let padding = (max.abs() * 0.08).max(1.0);
        return ((min - padding).max(0.0), max + padding);
    }
    let padding = (max - min) * 0.14;
    ((min - padding).max(0.0), max + padding)
}

fn scale_y(value: f64, min: f64, max: f64, plot_height: f64, height: f64, bottom: f64) -> f64 {
    let span = (max - min).max(1.0);
    height - bottom - ((value - min) / span * plot_height)
}

fn axis_tick(x: f64, y: f64, label: &str, class_name: &str, right_axis: bool) -> String {
    let tick_start = if right_axis { x } else { x - 6.0 };
    let tick_end = if right_axis { x + 6.0 } else { x };
    let label_x = if right_axis { x + 10.0 } else { x - 10.0 };
    let anchor = if right_axis { "start" } else { "end" };
    format!(
        "        <line x1=\"{tick_start:.1}\" y1=\"{y:.1}\" x2=\"{tick_end:.1}\" y2=\"{y:.1}\" class=\"axis-tick {class_name}\" />\n        <text x=\"{label_x:.1}\" y=\"{:.1}\" class=\"tick-label {class_name}\" text-anchor=\"{anchor}\">{}</text>\n",
        y + 4.0,
        escape_html(label)
    )
}

fn line_path(points: &[(f64, f64, &str, String)], class_name: &str) -> String {
    let mut path = String::new();
    for (index, (x, y, _, _)) in points.iter().enumerate() {
        if index == 0 {
            path.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            path.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }
    if path.is_empty() {
        String::new()
    } else {
        format!("        <path d=\"{path}\" class=\"{class_name}\" />\n")
    }
}

fn metric_chart_svg(points: &[ChartPoint], title: &str) -> String {
    const WIDTH: f64 = 840.0;
    const HEIGHT: f64 = 260.0;
    const TOP: f64 = 24.0;
    const RIGHT: f64 = 18.0;
    const BOTTOM: f64 = 54.0;
    const LEFT: f64 = 46.0;

    if points.is_empty() {
        return "      <div class=\"chart-empty\">No completed history</div>\n".to_string();
    }

    let max_total = points
        .iter()
        .map(|point| point.value)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let plot_width = WIDTH - LEFT - RIGHT;
    let plot_height = HEIGHT - TOP - BOTTOM;
    let bar_gap = 12.0;
    let bar_width = ((plot_width - bar_gap * (points.len().saturating_sub(1) as f64))
        / points.len() as f64)
        .max(12.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        "      <svg class=\"history-chart\" viewBox=\"0 0 840 260\" role=\"img\" aria-label=\"{}\">\n",
        escape_html(title)
    ));
    svg.push_str(&format!(
        "        <line x1=\"{LEFT}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"axis\" />\n",
        HEIGHT - BOTTOM,
        WIDTH - RIGHT,
        HEIGHT - BOTTOM
    ));
    svg.push_str(&format!(
        "        <line x1=\"{LEFT}\" y1=\"{TOP}\" x2=\"{LEFT}\" y2=\"{}\" class=\"axis\" />\n",
        HEIGHT - BOTTOM
    ));

    for (index, point) in points.iter().enumerate() {
        let bar_height = point.value / max_total * plot_height;
        let x = LEFT + index as f64 * (bar_width + bar_gap);
        let y = HEIGHT - BOTTOM - bar_height;
        svg.push_str(&format!(
            "        <rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{bar_width:.1}\" height=\"{bar_height:.1}\" class=\"bar\"><title>{}: {}</title></rect>\n",
            escape_html(&point.date),
            escape_html(&point.label)
        ));
        svg.push_str(&format!(
            "        <text x=\"{:.1}\" y=\"{:.1}\" class=\"bar-value\">{}</text>\n",
            x + bar_width / 2.0,
            y - 6.0,
            escape_html(&point.label)
        ));
        svg.push_str(&format!(
            "        <text x=\"{:.1}\" y=\"{}\" class=\"x-label\">{}</text>\n",
            x + bar_width / 2.0,
            HEIGHT - 28.0,
            escape_html(&point.date[..point.date.len().min(10)])
        ));
    }

    svg.push_str("      </svg>\n");
    svg
}

fn max_f64(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.fold(None, |max, value| {
        Some(max.map_or(value, |current: f64| current.max(value)))
    })
}

fn format_metric(value: f64, unit: &str) -> String {
    format!("{} {}", format_number(value), unit)
}

fn volume_unit_label(unit: WeightUnit) -> &'static str {
    match unit {
        WeightUnit::Kg => "kg-reps",
        WeightUnit::Lb => "lb-reps",
    }
}

fn format_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.001 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn default_timezone_label() -> String {
    env::var("TZ")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "local".to_string())
}

fn summary_item(label: &str, value: &str) -> String {
    format!(
        "        <div><dt>{}</dt><dd>{}</dd></div>\n",
        escape_html(label),
        escape_html(value)
    )
}

fn load_label(set: &SetEntry) -> Result<String> {
    match (set.weight_value, set.weight_unit) {
        (Some(weight), Some(unit)) => Ok(format!("{weight} {}", unit.as_str())),
        (None, None) => Ok("bodyweight".to_string()),
        _ => bail!("invalid stored set weight/unit pair"),
    }
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

const STYLES: &str = r#"  <style>
    :root {
      color-scheme: light;
      --ink: #1e2025;
      --muted: #626873;
      --line: #d9dde4;
      --panel: #f6f7f9;
      --accent: #0f766e;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      color: var(--ink);
      background: #ffffff;
      line-height: 1.45;
    }
    .page {
      max-width: 920px;
      margin: 0 auto;
      padding: 40px 24px 56px;
    }
    .session-header {
      border-bottom: 2px solid var(--ink);
      padding-bottom: 24px;
      margin-bottom: 28px;
    }
    .eyebrow {
      margin: 0 0 6px;
      color: var(--accent);
      font-weight: 700;
      letter-spacing: 0;
    }
    h1, h2, p, dl { margin-top: 0; }
    h1 {
      font-size: 32px;
      line-height: 1.15;
      margin-bottom: 18px;
    }
    .summary-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
      gap: 10px;
      margin-bottom: 16px;
    }
    .summary-grid div {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 10px 12px;
    }
    dt {
      color: var(--muted);
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
    }
    dd {
      margin: 3px 0 0;
      overflow-wrap: anywhere;
    }
    .session-notes,
    .entry-notes {
      color: var(--muted);
      margin-bottom: 0;
    }
    .exercise {
      break-inside: avoid;
      border-bottom: 1px solid var(--line);
      padding: 22px 0 26px;
    }
    .exercise-title-row {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      gap: 16px;
    }
    h2 {
      font-size: 22px;
      line-height: 1.25;
      margin-bottom: 8px;
    }
    .kind {
      color: var(--muted);
      font-size: 13px;
      white-space: nowrap;
    }
    .machine {
      margin-bottom: 12px;
    }
    .machine span {
      color: var(--muted);
    }
    .machine-notes {
      margin: 0 0 14px;
      padding-left: 20px;
      color: var(--muted);
    }
    table {
      width: 100%;
      border-collapse: collapse;
      border: 1px solid var(--line);
      font-size: 14px;
    }
    thead {
      background: var(--panel);
    }
    th, td {
      border-bottom: 1px solid var(--line);
      padding: 9px 8px;
      text-align: left;
    }
    tbody tr:last-child td {
      border-bottom: 0;
    }
    tbody.session-group {
      border-top: 2px solid var(--line);
    }
    tbody.session-group:first-of-type {
      border-top: 0;
    }
    .session-cell {
      background: #fbfcfd;
      vertical-align: top;
      font-weight: 600;
    }
    th {
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
    }
    .empty {
      color: var(--muted);
      text-align: center;
    }
    .history-chart {
      display: block;
      width: 100%;
      height: auto;
      border: 1px solid var(--line);
      background: #ffffff;
    }
    .axis {
      stroke: var(--line);
      stroke-width: 1.5;
    }
    .axis-tick {
      stroke: var(--line);
      stroke-width: 1.2;
    }
    .tick-label {
      fill: var(--muted);
      font-size: 11px;
    }
    .axis-right {
      stroke-dasharray: 4 4;
    }
    .axis-label {
      fill: var(--muted);
      font-size: 12px;
      font-weight: 700;
      text-anchor: start;
    }
    .axis-label-right {
      text-anchor: end;
    }
    .chart-legend text {
      fill: var(--muted);
      font-size: 12px;
    }
    .bar {
      fill: var(--accent);
    }
    .bar-value {
      fill: var(--ink);
      font-size: 13px;
      text-anchor: middle;
      font-weight: 700;
    }
    .volume-value {
      fill: var(--accent);
    }
    .volume-line {
      fill: none;
      stroke: var(--accent);
      stroke-width: 2.5;
    }
    .volume-dot {
      fill: var(--accent);
      stroke: #ffffff;
      stroke-width: 1.5;
    }
    .volume-tag {
      fill: var(--accent);
      font-size: 13px;
      text-anchor: middle;
      font-weight: 700;
      paint-order: stroke;
      stroke: #ffffff;
      stroke-width: 4px;
      stroke-linejoin: round;
    }
    .weight-line {
      fill: none;
      stroke: #9f1239;
      stroke-width: 2.5;
    }
    .weight-dot {
      fill: #9f1239;
      stroke: #ffffff;
      stroke-width: 1.5;
    }
    .weight-value {
      fill: #9f1239;
      font-size: 13px;
      text-anchor: middle;
      font-weight: 700;
    }
    .weight-tag {
      fill: #9f1239;
      font-size: 13px;
      text-anchor: middle;
      font-weight: 700;
      paint-order: stroke;
      stroke: #ffffff;
      stroke-width: 4px;
      stroke-linejoin: round;
    }
    .x-label {
      fill: var(--muted);
      font-size: 12px;
      text-anchor: middle;
    }
    .chart-empty {
      border: 1px solid var(--line);
      color: var(--muted);
      padding: 28px;
      text-align: center;
    }
    @media print {
      .page { padding: 24px 0; }
      .exercise { break-inside: avoid; }
    }
  </style>
"#;
