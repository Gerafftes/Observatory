//! Persistent metadata for Observatory experiments and the local replay MVP.
//!
//! SQLite stores only the experiment catalogue and hashes. Raw captures and
//! reports remain ordinary files so an experiment can be inspected without
//! turning the database into a binary-data store.

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::fs;

use crate::position_artifact::sha256_bytes;
use crate::calibration_persistence::{
    profile_context_sha256, CalibrationBundle, CalibrationSummary,
};

pub(crate) const SCHEMA_VERSION: i64 = 3;
pub(crate) const SUPPORTED_FIXTURE_ID: &str = "mmwave-synthetic-pass-status-v1";
pub(crate) const WORKFLOW_KIND: &str = "wifi_only_workflow";
pub(crate) const WORKFLOW_SOURCE: &str = "wifi_csi";
pub(crate) const PROFILE_SCHEMA_VERSION: u16 = 1;
const DEFAULT_SENSOR_MOUNT_RADIUS_M: f64 = 0.5;
const MAX_SENSOR_MOUNT_RADIUS_M: f64 = 5.0;
const MMWAVE_SENSOR: &str = "HLK-LD2450";

pub(crate) const WORKFLOW_PHASES: [&str; 10] = [
    "create_experiment",
    "seal_setup",
    "empty_calibration",
    "train_p01_p09",
    "randomize_blind_positions",
    "capture",
    "predict",
    "reveal_truth",
    "evaluate",
    "report",
];

const SYNTHETIC_FIXTURE: &str =
    include_str!("../../../../ui/tests/fixtures/mmwave-synthetic-pass-status.json");

static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExperimentArtifact {
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExperimentRun {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) source: String,
    pub(crate) fixture_id: String,
    pub(crate) state: String,
    pub(crate) phase: String,
    pub(crate) execution_status: String,
    pub(crate) validation_status: String,
    pub(crate) created_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) finished_at: Option<String>,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) artifacts: Vec<ExperimentArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) workflow: Option<ExperimentWorkflow>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SetupProfile {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) version: i64,
    pub(crate) revision_id: String,
    pub(crate) profile_sha256: String,
    pub(crate) profile_context_sha256: String,
    pub(crate) document: Value,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WorkflowPhaseEvent {
    pub(crate) id: i64,
    pub(crate) phase: String,
    pub(crate) status: String,
    pub(crate) payload: Value,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExperimentWorkflow {
    pub(crate) profile_id: String,
    pub(crate) profile_revision_id: Option<String>,
    pub(crate) profile_sha256: String,
    pub(crate) profile_context_sha256: Option<String>,
    pub(crate) dataset_version: String,
    pub(crate) firmware_version: String,
    pub(crate) calibration_id: Option<String>,
    pub(crate) calibration_source: Option<String>,
    pub(crate) calibration_context_sha256: Option<String>,
    pub(crate) blind_seed: u64,
    pub(crate) current_phase: String,
    pub(crate) current_status: String,
    pub(crate) events: Vec<WorkflowPhaseEvent>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExperimentStore {
    pool: SqlitePool,
    db_path: PathBuf,
}

impl ExperimentStore {
    pub(crate) async fn open(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir)
            .await
            .map_err(|error| format!("create experiment data directory: {error}"))?;

        let db_path = data_dir.join("observatory-experiments.sqlite3");
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|error| format!("open SQLite database: {error}"))?;

        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .map_err(|error| format!("enable SQLite foreign keys: {error}"))?;

        migrate(&pool).await?;

        Ok(Self { pool, db_path })
    }

    pub(crate) fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub(crate) async fn run_count(&self) -> Result<i64, String> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM experiment_runs")
            .fetch_one(&self.pool)
            .await
            .map_err(|error| format!("count experiment runs: {error}"))
    }

    pub(crate) async fn list_profiles(&self) -> Result<Vec<SetupProfile>, String> {
        let rows = sqlx::query(
            "SELECT p.id, p.label, p.version, p.profile_sha256, p.document_json,
                    p.created_at, p.updated_at, r.revision_id, r.profile_context_sha256
             FROM setup_profiles p
             LEFT JOIN setup_profile_revisions r
               ON r.profile_id = p.id AND r.version = p.version
             ORDER BY p.updated_at DESC, p.id DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| format!("list setup profiles: {error}"))?;

        rows.iter().map(profile_from_row).collect()
    }

    pub(crate) async fn get_profile(&self, id: &str) -> Result<Option<SetupProfile>, String> {
        let row = sqlx::query(
            "SELECT p.id, p.label, p.version, p.profile_sha256, p.document_json,
                    p.created_at, p.updated_at, r.revision_id, r.profile_context_sha256
             FROM setup_profiles p
             LEFT JOIN setup_profile_revisions r
               ON r.profile_id = p.id AND r.version = p.version
             WHERE p.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("get setup profile: {error}"))?;

        row.as_ref().map(profile_from_row).transpose()
    }

    pub(crate) async fn get_profile_revision(
        &self,
        profile_id: &str,
        revision_id: &str,
    ) -> Result<Option<SetupProfile>, String> {
        let row = sqlx::query(
            "SELECT revision_id, profile_id, label, version, profile_sha256,
                    profile_context_sha256, document_json, created_at,
                    created_at AS updated_at
             FROM setup_profile_revisions
             WHERE profile_id = ? AND revision_id = ?",
        )
        .bind(profile_id)
        .bind(revision_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("get setup profile revision: {error}"))?;

        row.as_ref().map(profile_revision_from_row).transpose()
    }

    pub(crate) async fn create_profile(
        &self,
        label: &str,
        document: &Value,
    ) -> Result<SetupProfile, String> {
        let label = validate_label(label, "profile label")?;
        let document = normalize_profile_document(document)?;
        let profile_sha256 = profile_hash(&document)?;
        let profile_context_sha256 = profile_context_sha256(&document)?;
        let id = new_profile_id();
        let revision_id = new_profile_revision_id(&id, 1);
        let now = timestamp();
        let document_json = serde_json::to_string(&document)
            .map_err(|error| format!("serialize setup profile document: {error}"))?;
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin setup profile creation: {error}"))?;

        sqlx::query(
            "INSERT INTO setup_profiles
             (id, label, version, profile_sha256, document_json, created_at, updated_at)
             VALUES (?, ?, 1, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(label)
        .bind(&profile_sha256)
        .bind(&document_json)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create setup profile: {error}"))?;

        sqlx::query(
            "INSERT INTO setup_profile_revisions
             (revision_id, profile_id, version, label, profile_sha256,
              profile_context_sha256, document_json, created_at)
             VALUES (?, ?, 1, ?, ?, ?, ?, ?)",
        )
        .bind(&revision_id)
        .bind(&id)
        .bind(label)
        .bind(&profile_sha256)
        .bind(&profile_context_sha256)
        .bind(&document_json)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create setup profile revision: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit setup profile creation: {error}"))?;

        self.get_profile(&id)
            .await?
            .ok_or_else(|| "created setup profile could not be read back".to_string())
    }

    pub(crate) async fn update_profile(
        &self,
        id: &str,
        label: &str,
        document: &Value,
    ) -> Result<SetupProfile, String> {
        let label = validate_label(label, "profile label")?;
        let document = normalize_profile_document(document)?;
        let profile_sha256 = profile_hash(&document)?;
        let profile_context_sha256 = profile_context_sha256(&document)?;
        let document_json = serde_json::to_string(&document)
            .map_err(|error| format!("serialize setup profile document: {error}"))?;
        let current = self
            .get_profile(id)
            .await?
            .ok_or_else(|| "setup profile not found".to_string())?;
        if current.label == label && current.profile_sha256 == profile_sha256 {
            return Ok(current);
        }
        let next_version = current.version + 1;
        let revision_id = new_profile_revision_id(id, next_version);
        let now = timestamp();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin setup profile update: {error}"))?;

        // Preserve the pre-v3 current row if a database was upgraded from an
        // older build that had no revision snapshot yet.
        sqlx::query(
            "INSERT OR IGNORE INTO setup_profile_revisions
             (revision_id, profile_id, version, label, profile_sha256,
              profile_context_sha256, document_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&current.revision_id)
        .bind(&current.id)
        .bind(current.version)
        .bind(&current.label)
        .bind(&current.profile_sha256)
        .bind(&current.profile_context_sha256)
        .bind(serde_json::to_string(&current.document).map_err(|error| {
            format!("serialize previous setup profile document: {error}")
        })?)
        .bind(&current.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("preserve setup profile revision: {error}"))?;

        let result = sqlx::query(
            "UPDATE setup_profiles
             SET label = ?, version = ?, profile_sha256 = ?,
                 document_json = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(label)
        .bind(next_version)
        .bind(&profile_sha256)
        .bind(&document_json)
        .bind(&now)
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("update setup profile: {error}"))?;
        if result.rows_affected() != 1 {
            transaction.rollback().await.ok();
            return Err("setup profile not found".to_string());
        }

        sqlx::query(
            "INSERT INTO setup_profile_revisions
             (revision_id, profile_id, version, label, profile_sha256,
              profile_context_sha256, document_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&revision_id)
        .bind(id)
        .bind(next_version)
        .bind(label)
        .bind(&profile_sha256)
        .bind(&profile_context_sha256)
        .bind(&document_json)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create updated setup profile revision: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit setup profile update: {error}"))?;

        self.get_profile(id)
            .await?
            .ok_or_else(|| "updated setup profile could not be read back".to_string())
    }

    pub(crate) async fn create_workflow_run(
        &self,
        label: &str,
        profile_id: &str,
        profile_revision_id: Option<&str>,
        dataset_version: &str,
        firmware_version: &str,
        blind_seed: u64,
    ) -> Result<ExperimentRun, String> {
        let label = validate_label(label, "experiment label")?;
        let profile = match profile_revision_id {
            Some(revision_id) => self
                .get_profile_revision(profile_id, revision_id)
                .await?
                .ok_or_else(|| "setup profile revision not found".to_string())?,
            None => self
                .get_profile(profile_id)
                .await?
                .ok_or_else(|| "setup profile not found".to_string())?,
        };
        let dataset_version = validate_short_identity(dataset_version, "dataset_version")?;
        let firmware_version = validate_short_identity(firmware_version, "firmware_version")?;
        let id = new_run_id();
        let now = timestamp();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin workflow creation: {error}"))?;

        sqlx::query(
            "INSERT INTO experiment_runs
             (id, label, kind, source, fixture_id, state, phase,
              execution_status, validation_status, created_at)
             VALUES (?, ?, ?, ?, 'none', 'created', ?, 'NOT_RUN', 'UNVALIDATED', ?)",
        )
        .bind(&id)
        .bind(label)
        .bind(WORKFLOW_KIND)
        .bind(WORKFLOW_SOURCE)
        .bind(WORKFLOW_PHASES[0])
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("insert workflow run: {error}"))?;

        sqlx::query(
            "INSERT INTO experiment_workflows
             (run_id, profile_id, profile_revision_id, profile_sha256,
              profile_context_sha256, dataset_version, firmware_version,
              calibration_id, calibration_source, calibration_context_sha256,
              blind_seed, current_phase, current_status)
             VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, ?, ?, 'READY')",
        )
        .bind(&id)
        .bind(&profile.id)
        .bind(&profile.revision_id)
        .bind(&profile.profile_sha256)
        .bind(&profile.profile_context_sha256)
        .bind(dataset_version)
        .bind(firmware_version)
        .bind(i64::try_from(blind_seed).map_err(|_| "blind_seed is too large".to_string())?)
        .bind(WORKFLOW_PHASES[0])
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("insert workflow metadata: {error}"))?;

        insert_phase_event(&mut transaction, &id, WORKFLOW_PHASES[0], "READY", &json!({})).await?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit workflow creation: {error}"))?;

        self.get_run(&id)
            .await?
            .ok_or_else(|| "created workflow run could not be read back".to_string())
    }

    pub(crate) async fn advance_workflow(
        &self,
        run_id: &str,
        phase: &str,
        status: &str,
        payload: &Value,
    ) -> Result<ExperimentRun, String> {
        validate_workflow_phase(phase)?;
        validate_phase_status(status)?;
        let payload = bounded_payload(payload)?;
        let workflow = self
            .load_workflow(run_id)
            .await?
            .ok_or_else(|| "workflow run not found".to_string())?;
        let current_index = phase_index(&workflow.current_phase)
            .ok_or_else(|| "workflow has an invalid current phase".to_string())?;
        let requested_index = phase_index(phase).expect("phase validated above");
        if requested_index > current_index + 1 || requested_index < current_index {
            return Err(format!(
                "workflow phase must stay at {} or advance to {}; got {phase}",
                workflow.current_phase,
                WORKFLOW_PHASES[(current_index + 1).min(WORKFLOW_PHASES.len() - 1)]
            ));
        }
        if matches!(workflow.current_status.as_str(), "PASS" | "REUSED")
            && requested_index == current_index
        {
            return Err(format!("workflow phase {phase} is already complete"));
        }

        let calibration_id = payload
            .get("calibration_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let calibration_source = payload
            .get("calibration_source")
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "captured" | "reused"));
        let calibration_context_sha256 = payload
            .get("calibration_context_sha256")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        if phase == "empty_calibration" && status == "REUSED" {
            if calibration_id.is_none()
                || calibration_source != Some("reused")
                || calibration_context_sha256.is_none()
            {
                return Err(
                    "reused empty calibration needs calibration_id, source=reused, and a context hash"
                        .to_string(),
                );
            }
        }

        let (run_state, execution_status, finished_at) = if phase == "report" && status == "PASS" {
            ("completed", "PASS", Some(timestamp()))
        } else if matches!(status, "BLOCKED" | "ERROR") {
            ("failed", "ERROR", Some(timestamp()))
        } else if status == "RUNNING" {
            ("running", "RUNNING", None)
        } else {
            ("running", "RUNNING", None)
        };

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin workflow phase update: {error}"))?;
        let result = sqlx::query(
            "UPDATE experiment_workflows
             SET current_phase = ?, current_status = ?,
                 calibration_id = COALESCE(?, calibration_id),
                 calibration_source = COALESCE(?, calibration_source),
                 calibration_context_sha256 = COALESCE(?, calibration_context_sha256)
             WHERE run_id = ?",
        )
        .bind(phase)
        .bind(status)
        .bind(calibration_id)
        .bind(calibration_source)
        .bind(calibration_context_sha256)
        .bind(run_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("update workflow phase: {error}"))?;
        if result.rows_affected() != 1 {
            return Err("workflow run not found".to_string());
        }
        insert_phase_event(&mut transaction, run_id, phase, status, &payload).await?;
        sqlx::query(
            "UPDATE experiment_runs
             SET state = ?, phase = ?, execution_status = ?,
                 started_at = COALESCE(started_at, ?), finished_at = ?,
                 error_code = ?, error_message = ?
             WHERE id = ? AND kind = ?",
        )
        .bind(run_state)
        .bind(phase)
        .bind(execution_status)
        .bind(timestamp())
        .bind(finished_at)
        .bind((status == "BLOCKED").then_some("PHASE_BLOCKED"))
        .bind((status == "BLOCKED").then_some(
            payload
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("workflow phase is blocked"),
        ))
        .bind(run_id)
        .bind(WORKFLOW_KIND)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("update workflow run state: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit workflow phase update: {error}"))?;

        self.get_run(run_id)
            .await?
            .ok_or_else(|| "updated workflow run could not be read back".to_string())
    }

    pub(crate) async fn write_workflow_report(
        &self,
        run_id: &str,
    ) -> Result<ExperimentRun, String> {
        let run = self
            .get_run(run_id)
            .await?
            .ok_or_else(|| "workflow run not found".to_string())?;
        let Some(workflow) = run.workflow.as_ref() else {
            return Err("run is not a WiFi-only workflow".to_string());
        };
        if workflow.current_phase != "report" || workflow.current_status != "PASS" {
            return Err("report phase must be marked PASS before writing a report".to_string());
        }

        let report = json!({
            "report_version": 2,
            "kind": WORKFLOW_KIND,
            "run_id": run.id,
            "label": run.label,
            "source": WORKFLOW_SOURCE,
            "profile_id": workflow.profile_id,
            "profile_revision_id": workflow.profile_revision_id,
            "profile_sha256": workflow.profile_sha256,
            "profile_context_sha256": workflow.profile_context_sha256,
            "dataset_version": workflow.dataset_version,
            "firmware_version": workflow.firmware_version,
            "blind_seed": workflow.blind_seed,
            "calibration_id": workflow.calibration_id,
            "calibration_source": workflow.calibration_source,
            "calibration_context_sha256": workflow.calibration_context_sha256,
            "artifacts": run.artifacts,
            "execution_status": "PASS",
            "validation_status": "UNVALIDATED",
            "live_position_approved": false,
            "mmwave_evidence": false,
            "phases": workflow.events,
            "metrics": {
                "accuracy": workflow_metric(workflow, "accuracy"),
                "coverage": workflow_metric(workflow, "coverage"),
                "error_distance_m": workflow_metric(workflow, "error_distance_m"),
                "packet_loss": workflow_metric(workflow, "packet_loss")
            },
            "limitations": [
                "The report is not a blind validation verdict until the sealed WiFi capture and truth reveal are complete.",
                "The mmWave reference path is intentionally absent from this WiFi-only workflow.",
                "Live position remains locked until the existing position acceptance gates pass."
            ]
        });
        let bytes = serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("serialize workflow report: {error}"))?;
        let relative_path = format!("experiments/{run_id}/report.json");
        let report_path = self
            .db_path
            .parent()
            .unwrap_or_else(|| Path::new("data"))
            .join(&relative_path);
        if let Some(parent) = report_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| format!("create workflow report directory: {error}"))?;
        }
        fs::write(&report_path, &bytes)
            .await
            .map_err(|error| format!("write workflow report: {error}"))?;

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin workflow report registration: {error}"))?;
        sqlx::query(
            "UPDATE experiment_runs
             SET state = 'completed', phase = 'report', execution_status = 'PASS',
                 validation_status = 'UNVALIDATED', finished_at = ?
             WHERE id = ? AND kind = ?",
        )
        .bind(timestamp())
        .bind(run_id)
        .bind(WORKFLOW_KIND)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("complete workflow report: {error}"))?;
        sqlx::query("DELETE FROM experiment_artifacts WHERE run_id = ? AND kind = 'report'")
            .bind(run_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("clear workflow report artifacts: {error}"))?;
        sqlx::query(
            "INSERT INTO experiment_artifacts
             (run_id, kind, relative_path, sha256, created_at)
             VALUES (?, 'report', ?, ?, ?)",
        )
        .bind(run_id)
        .bind(&relative_path)
        .bind(sha256_bytes(&bytes))
        .bind(timestamp())
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("register workflow report artifact: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit workflow report registration: {error}"))?;

        self.get_run(run_id)
            .await?
            .ok_or_else(|| "completed workflow run could not be read back".to_string())
    }

    pub(crate) async fn register_workflow_artifact(
        &self,
        run_id: &str,
        kind: &str,
        relative_path: &str,
    ) -> Result<ExperimentRun, String> {
        validate_workflow_artifact_kind(kind)?;
        let relative_path = validate_relative_artifact_path(relative_path)?;
        let run = self
            .get_run(run_id)
            .await?
            .ok_or_else(|| "workflow run not found".to_string())?;
        if run.workflow.is_none() {
            return Err("run is not a WiFi-only workflow".to_string());
        }

        let data_root = self
            .db_path
            .parent()
            .unwrap_or_else(|| Path::new("data"));
        let root = fs::canonicalize(data_root)
            .await
            .map_err(|error| format!("resolve experiment data directory: {error}"))?;
        let path = data_root.join(&relative_path);
        let canonical_path = fs::canonicalize(&path)
            .await
            .map_err(|error| format!("artifact file is not readable: {error}"))?;
        if !canonical_path.starts_with(&root) {
            return Err("artifact path must stay inside the experiment data directory".to_string());
        }
        let metadata = fs::metadata(&canonical_path)
            .await
            .map_err(|error| format!("inspect artifact file: {error}"))?;
        if !metadata.is_file() {
            return Err("artifact path must point to a regular file".to_string());
        }
        let bytes = fs::read(&canonical_path)
            .await
            .map_err(|error| format!("read artifact file: {error}"))?;
        let sha256 = sha256_bytes(&bytes);
        let relative_path_string = relative_path.to_string_lossy().into_owned();
        let created_at = timestamp();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin artifact registration: {error}"))?;
        sqlx::query("DELETE FROM experiment_artifacts WHERE run_id = ? AND kind = ?")
            .bind(run_id)
            .bind(kind)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("replace workflow artifact: {error}"))?;
        sqlx::query(
            "INSERT INTO experiment_artifacts
             (run_id, kind, relative_path, sha256, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(run_id)
        .bind(kind)
        .bind(relative_path_string)
        .bind(sha256)
        .bind(created_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("register workflow artifact: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit artifact registration: {error}"))?;

        self.get_run(run_id)
            .await?
            .ok_or_else(|| "registered workflow artifact run could not be read back".to_string())
    }

    pub(crate) async fn report_json(&self, run_id: &str) -> Result<Option<Value>, String> {
        let Some(run) = self.get_run(run_id).await? else {
            return Ok(None);
        };
        let Some(artifact) = run.artifacts.iter().find(|artifact| artifact.kind == "report")
        else {
            return Ok(None);
        };
        let path = self
            .db_path
            .parent()
            .unwrap_or_else(|| Path::new("data"))
            .join(&artifact.relative_path);
        let bytes = fs::read(path)
            .await
            .map_err(|error| format!("read experiment report: {error}"))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("decode experiment report: {error}"))
    }

    pub(crate) async fn persist_calibration_bundle(
        &self,
        bundle: &CalibrationBundle,
    ) -> Result<CalibrationSummary, String> {
        bundle.validate()?;
        let data_root = self
            .db_path
            .parent()
            .unwrap_or_else(|| Path::new("data"));
        let calibration_dir = data_root.join("calibrations");
        fs::create_dir_all(&calibration_dir)
            .await
            .map_err(|error| format!("create calibration directory: {error}"))?;
        let bytes = serde_json::to_vec_pretty(bundle)
            .map_err(|error| format!("serialize calibration bundle: {error}"))?;
        let relative_path = format!("calibrations/{}.json", bundle.calibration_id);
        let path = data_root.join(&relative_path);
        let temporary_path = data_root.join(format!(".{}.tmp", bundle.calibration_id));
        fs::write(&temporary_path, &bytes)
            .await
            .map_err(|error| format!("write temporary calibration bundle: {error}"))?;
        if let Err(error) = fs::rename(&temporary_path, &path).await {
            let _ = fs::remove_file(&temporary_path).await;
            return Err(format!("commit calibration bundle: {error}"));
        }

        let sha256 = sha256_bytes(&bytes);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin calibration registration: {error}"))?;
        sqlx::query(
            "INSERT INTO calibration_bundles
             (calibration_id, profile_id, profile_revision_id, profile_sha256,
              profile_context_sha256, setup_id, setup_sha256,
              calibration_context_sha256, algorithm_version, relative_path,
              sha256, captured_at, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'READY')",
        )
        .bind(&bundle.calibration_id)
        .bind(&bundle.profile_id)
        .bind(&bundle.profile_revision_id)
        .bind(&bundle.profile_sha256)
        .bind(&bundle.profile_context_sha256)
        .bind(&bundle.setup_id)
        .bind(&bundle.setup_sha256)
        .bind(&bundle.calibration_context_sha256)
        .bind(&bundle.algorithm_version)
        .bind(&relative_path)
        .bind(&sha256)
        .bind(&bundle.captured_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("register calibration bundle: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit calibration registration: {error}"))?;
        Ok(bundle.summary())
    }

    pub(crate) async fn find_compatible_calibration(
        &self,
        profile_id: &str,
        profile_context_sha256: &str,
        setup_id: &str,
        setup_sha256: &str,
    ) -> Result<Option<CalibrationBundle>, String> {
        let row = sqlx::query(
            "SELECT calibration_id, profile_id, profile_revision_id,
                    profile_sha256, profile_context_sha256, setup_id,
                    setup_sha256, calibration_context_sha256,
                    algorithm_version, relative_path, sha256, captured_at, status
             FROM calibration_bundles
             WHERE profile_id = ? AND profile_context_sha256 = ?
               AND setup_id = ? AND setup_sha256 = ? AND status = 'READY'
             ORDER BY captured_at DESC, calibration_id DESC
             LIMIT 1",
        )
        .bind(profile_id)
        .bind(profile_context_sha256)
        .bind(setup_id)
        .bind(setup_sha256)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("find compatible calibration: {error}"))?;

        let Some(row) = row else {
            return Ok(None);
        };
        self.load_calibration_row(&row).await.map(Some)
    }

    pub(crate) async fn latest_calibration_for_setup(
        &self,
        setup_id: &str,
        setup_sha256: &str,
    ) -> Result<Option<CalibrationBundle>, String> {
        let row = sqlx::query(
            "SELECT calibration_id, profile_id, profile_revision_id,
                    profile_sha256, profile_context_sha256, setup_id,
                    setup_sha256, calibration_context_sha256,
                    algorithm_version, relative_path, sha256, captured_at, status
             FROM calibration_bundles
             WHERE setup_id = ? AND setup_sha256 = ? AND status = 'READY'
             ORDER BY captured_at DESC, calibration_id DESC
             LIMIT 1",
        )
        .bind(setup_id)
        .bind(setup_sha256)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("find latest setup calibration: {error}"))?;

        let Some(row) = row else {
            return Ok(None);
        };
        self.load_calibration_row(&row).await.map(Some)
    }

    async fn load_calibration_row(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<CalibrationBundle, String> {
        let relative_path: String = row.get("relative_path");
        let relative_path = validate_relative_artifact_path(&relative_path)?;
        let data_root = self
            .db_path
            .parent()
            .unwrap_or_else(|| Path::new("data"));
        let root = fs::canonicalize(data_root)
            .await
            .map_err(|error| format!("resolve calibration data directory: {error}"))?;
        let path = data_root.join(&relative_path);
        let canonical_path = fs::canonicalize(&path)
            .await
            .map_err(|error| format!("calibration bundle is not readable: {error}"))?;
        if !canonical_path.starts_with(&root) {
            return Err("calibration bundle path must stay inside the data directory".to_string());
        }
        let metadata = fs::metadata(&canonical_path)
            .await
            .map_err(|error| format!("inspect calibration bundle: {error}"))?;
        if !metadata.is_file() {
            return Err("calibration bundle must be a regular file".to_string());
        }
        let bytes = fs::read(&canonical_path)
            .await
            .map_err(|error| format!("read calibration bundle: {error}"))?;
        let expected_sha256: String = row.get("sha256");
        let actual_sha256 = sha256_bytes(&bytes);
        if actual_sha256 != expected_sha256 {
            return Err(format!(
                "calibration bundle hash mismatch: expected {expected_sha256}, got {actual_sha256}"
            ));
        }
        let bundle: CalibrationBundle = serde_json::from_slice(&bytes)
            .map_err(|error| format!("decode calibration bundle: {error}"))?;
        bundle.validate()?;
        for (column, actual) in [
            ("calibration_id", &bundle.calibration_id),
            ("profile_id", &bundle.profile_id),
            ("profile_revision_id", &bundle.profile_revision_id),
            ("profile_sha256", &bundle.profile_sha256),
            ("profile_context_sha256", &bundle.profile_context_sha256),
            ("setup_id", &bundle.setup_id),
            ("setup_sha256", &bundle.setup_sha256),
            ("calibration_context_sha256", &bundle.calibration_context_sha256),
            ("algorithm_version", &bundle.algorithm_version),
            ("captured_at", &bundle.captured_at),
        ] {
            let expected: String = row.get(column);
            if expected != *actual {
                return Err(format!("calibration metadata mismatch in {column}"));
            }
        }
        Ok(bundle)
    }

    pub(crate) async fn create_run(
        &self,
        label: &str,
        fixture_id: &str,
    ) -> Result<ExperimentRun, String> {
        let label = label.trim();
        if label.is_empty() || label.len() > 120 {
            return Err("label must contain between 1 and 120 characters".to_string());
        }
        if fixture_id != SUPPORTED_FIXTURE_ID {
            return Err(format!("unsupported fixture_id: {fixture_id}"));
        }

        let id = new_run_id();
        let now = timestamp();
        sqlx::query(
            "INSERT INTO experiment_runs
             (id, label, kind, source, fixture_id, state, phase,
              execution_status, validation_status, created_at)
             VALUES (?, ?, 'synthetic_replay', 'synthetic_replay', ?,
                     'created', 'created', 'NOT_RUN', 'UNVALIDATED', ?)",
        )
        .bind(&id)
        .bind(label)
        .bind(fixture_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|error| format!("create experiment run: {error}"))?;

        self.get_run(&id)
            .await?
            .ok_or_else(|| "created experiment run could not be read back".to_string())
    }

    pub(crate) async fn list_runs(&self, limit: u32) -> Result<Vec<ExperimentRun>, String> {
        let limit = i64::from(limit.clamp(1, 100));
        let rows = sqlx::query(
            "SELECT id, label, kind, source, fixture_id, state, phase,
                    execution_status, validation_status, created_at,
                    started_at, finished_at, error_code, error_message
             FROM experiment_runs
             ORDER BY created_at DESC, id DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| format!("list experiment runs: {error}"))?;

        let mut runs = Vec::with_capacity(rows.len());
        for row in rows {
            let mut run = run_from_row(&row);
            run.artifacts = self.list_artifacts(&run.id).await?;
            run.workflow = self.load_workflow(&run.id).await?;
            runs.push(run);
        }
        Ok(runs)
    }

    pub(crate) async fn get_run(&self, id: &str) -> Result<Option<ExperimentRun>, String> {
        let row = sqlx::query(
            "SELECT id, label, kind, source, fixture_id, state, phase,
                    execution_status, validation_status, created_at,
                    started_at, finished_at, error_code, error_message
             FROM experiment_runs WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("get experiment run: {error}"))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut run = run_from_row(&row);
        run.artifacts = self.list_artifacts(&run.id).await?;
        run.workflow = self.load_workflow(&run.id).await?;
        Ok(Some(run))
    }

    async fn load_workflow(&self, run_id: &str) -> Result<Option<ExperimentWorkflow>, String> {
        let row = sqlx::query(
            "SELECT profile_id, profile_revision_id, profile_sha256,
                    profile_context_sha256, dataset_version, firmware_version,
                    calibration_id, calibration_source, calibration_context_sha256,
                    blind_seed, current_phase, current_status
             FROM experiment_workflows WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| format!("load workflow metadata: {error}"))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let event_rows = sqlx::query(
            "SELECT id, phase, status, payload_json, created_at
             FROM experiment_phase_events WHERE run_id = ? ORDER BY id ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| format!("load workflow phase events: {error}"))?;
        let mut events = Vec::with_capacity(event_rows.len());
        for event_row in event_rows {
            let payload_json: String = event_row.get("payload_json");
            let payload = serde_json::from_str(&payload_json)
                .map_err(|error| format!("decode workflow phase payload: {error}"))?;
            events.push(WorkflowPhaseEvent {
                id: event_row.get("id"),
                phase: event_row.get("phase"),
                status: event_row.get("status"),
                payload,
                created_at: event_row.get("created_at"),
            });
        }
        Ok(Some(ExperimentWorkflow {
            profile_id: row.get("profile_id"),
            profile_revision_id: row.try_get("profile_revision_id").unwrap_or(None),
            profile_sha256: row.get("profile_sha256"),
            profile_context_sha256: row.try_get("profile_context_sha256").unwrap_or(None),
            dataset_version: row.get("dataset_version"),
            firmware_version: row.get("firmware_version"),
            calibration_id: row.try_get("calibration_id").unwrap_or(None),
            calibration_source: row.try_get("calibration_source").unwrap_or(None),
            calibration_context_sha256: row
                .try_get("calibration_context_sha256")
                .unwrap_or(None),
            blind_seed: u64::try_from(row.get::<i64, _>("blind_seed")).unwrap_or(0),
            current_phase: row.get("current_phase"),
            current_status: row.get("current_status"),
            events,
        }))
    }

    async fn list_artifacts(&self, run_id: &str) -> Result<Vec<ExperimentArtifact>, String> {
        let rows = sqlx::query(
            "SELECT id, kind, relative_path, sha256, created_at
             FROM experiment_artifacts WHERE run_id = ? ORDER BY id ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| format!("list experiment artifacts: {error}"))?;

        Ok(rows
            .iter()
            .map(|row| ExperimentArtifact {
                id: row.get("id"),
                kind: row.get("kind"),
                relative_path: row.get("relative_path"),
                sha256: row.get("sha256"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn mark_running(&self, id: &str) -> Result<ExperimentRun, String> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin replay transaction: {error}"))?;

        sqlx::query("DELETE FROM experiment_artifacts WHERE run_id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("clear replay artifacts: {error}"))?;

        let result = sqlx::query(
            "UPDATE experiment_runs
             SET state = 'running', phase = 'replay', execution_status = 'RUNNING',
                 started_at = ?, finished_at = NULL, error_code = NULL,
                 error_message = NULL
             WHERE id = ? AND state IN ('created', 'failed')",
        )
        .bind(timestamp())
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("mark experiment run running: {error}"))?;

        if result.rows_affected() != 1 {
            return Err("experiment run is already running or completed".to_string());
        }

        transaction
            .commit()
            .await
            .map_err(|error| format!("commit replay start: {error}"))?;

        self.get_run(id)
            .await?
            .ok_or_else(|| "running experiment run could not be read back".to_string())
    }

    async fn complete_run(
        &self,
        id: &str,
        artifacts: &[ArtifactInput],
    ) -> Result<ExperimentRun, String> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| format!("begin replay completion: {error}"))?;
        let result = sqlx::query(
            "UPDATE experiment_runs
             SET state = 'completed', phase = 'report', execution_status = 'PASS',
                 validation_status = 'UNVALIDATED', finished_at = ?
             WHERE id = ? AND state = 'running'",
        )
        .bind(timestamp())
        .bind(id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("complete experiment run: {error}"))?;

        if result.rows_affected() != 1 {
            return Err("experiment run is not in the running state".to_string());
        }

        for artifact in artifacts {
            sqlx::query(
                "INSERT INTO experiment_artifacts
                 (run_id, kind, relative_path, sha256, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&artifact.kind)
            .bind(&artifact.relative_path)
            .bind(&artifact.sha256)
            .bind(timestamp())
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("register experiment artifact: {error}"))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| format!("commit replay completion: {error}"))?;

        self.get_run(id)
            .await?
            .ok_or_else(|| "completed experiment run could not be read back".to_string())
    }

    async fn mark_failed(&self, id: &str, error_code: &str, message: &str) {
        let _ = sqlx::query(
            "UPDATE experiment_runs
             SET state = 'failed', phase = 'failed', execution_status = 'ERROR',
                 finished_at = ?, error_code = ?, error_message = ?
             WHERE id = ? AND state = 'running'",
        )
        .bind(timestamp())
        .bind(error_code)
        .bind(message)
        .bind(id)
        .execute(&self.pool)
        .await;
    }
}

#[derive(Debug, Clone)]
struct ArtifactInput {
    kind: String,
    relative_path: String,
    sha256: String,
}

pub(crate) async fn replay_run(
    store: &ExperimentStore,
    run_id: &str,
) -> Result<ExperimentRun, String> {
    let Some(existing) = store.get_run(run_id).await? else {
        return Err("experiment run not found".to_string());
    };
    if existing.state == "completed" {
        return Err("completed experiment runs cannot be replayed again".to_string());
    }
    if existing.fixture_id != SUPPORTED_FIXTURE_ID {
        return Err(format!("unsupported fixture_id: {}", existing.fixture_id));
    }

    let running = store.mark_running(run_id).await?;
    let result = write_synthetic_report(store, &running).await;
    match result {
        Ok(artifact) => store.complete_run(run_id, &[artifact]).await,
        Err(error) => {
            store.mark_failed(run_id, "REPLAY_FAILED", &error).await;
            Err(error)
        }
    }
}

async fn write_synthetic_report(
    store: &ExperimentStore,
    run: &ExperimentRun,
) -> Result<ArtifactInput, String> {
    let report = synthetic_report(run);
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("serialize replay report: {error}"))?;
    let relative_path = format!("experiments/{}/report.json", run.id);
    let report_path = store
        .db_path()
        .parent()
        .unwrap_or_else(|| Path::new("data"))
        .join(&relative_path);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("create replay artifact directory: {error}"))?;
    }
    fs::write(&report_path, &bytes)
        .await
        .map_err(|error| format!("write replay report: {error}"))?;

    Ok(ArtifactInput {
        kind: "report".to_string(),
        relative_path,
        sha256: sha256_bytes(&bytes),
    })
}

fn synthetic_report(run: &ExperimentRun) -> Value {
    let fixture_sha256 = sha256_bytes(SYNTHETIC_FIXTURE.as_bytes());
    let phases = [
        ("create_experiment", "SIMULATED_PASS"),
        ("seal_setup", "SIMULATED_PASS"),
        ("empty_calibration", "SIMULATED_PASS"),
        ("train_p01_p09", "SIMULATED_PASS"),
        ("randomize_blind_positions", "SIMULATED_PASS"),
        ("capture_replay", "SIMULATED_PASS"),
        ("predict", "SIMULATED_PASS"),
        ("reveal_truth", "SIMULATED_PASS"),
        ("evaluate", "SIMULATED_PASS"),
        ("report", "PASS"),
    ];

    json!({
        "report_version": 1,
        "run_id": run.id,
        "label": run.label,
        "kind": "synthetic_replay",
        "source": "synthetic_replay",
        "fixture_id": SUPPORTED_FIXTURE_ID,
        "fixture_sha256": fixture_sha256,
        "software_version": env!("CARGO_PKG_VERSION"),
        "execution_status": "PASS",
        "software_test_status": "PASS",
        "software_test_scope": "deterministic fixture contract; no live sensor transport",
        "validation_status": "UNVALIDATED",
        "live_position_approved": false,
        "live_hardware_evidence": false,
        "hardware_actions": [],
        "phases": phases.into_iter().map(|(name, status)| json!({
            "name": name,
            "status": status,
            "evidence": "synthetic_fixture"
        })).collect::<Vec<_>>(),
        "metrics": {
            "coverage": "synthetic_only",
            "accuracy": null,
            "error_distance_m": null,
            "packet_loss": null
        },
        "limitations": [
            "No CSI packets were captured.",
            "No mmWave packets were received.",
            "No live position may be enabled from this run."
        ]
    })
}

fn run_from_row(row: &sqlx::sqlite::SqliteRow) -> ExperimentRun {
    ExperimentRun {
        id: row.get("id"),
        label: row.get("label"),
        kind: row.get("kind"),
        source: row.get("source"),
        fixture_id: row.get("fixture_id"),
        state: row.get("state"),
        phase: row.get("phase"),
        execution_status: row.get("execution_status"),
        validation_status: row.get("validation_status"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        error_code: row.get("error_code"),
        error_message: row.get("error_message"),
        artifacts: Vec::new(),
        workflow: None,
    }
}

fn profile_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SetupProfile, String> {
    let document_json: String = row.get("document_json");
    let document = serde_json::from_str(&document_json)
        .map_err(|error| format!("decode setup profile document: {error}"))?;
    let id: String = row.get("id");
    let version: i64 = row.get("version");
    let revision_id = row
        .try_get::<Option<String>, _>("revision_id")
        .unwrap_or(None)
        .unwrap_or_else(|| new_profile_revision_id(&id, version));
    let profile_context_sha256 = row
        .try_get::<Option<String>, _>("profile_context_sha256")
        .unwrap_or(None)
        .filter(|value| !value.is_empty())
        .unwrap_or(profile_context_sha256(&document)?);
    Ok(SetupProfile {
        id,
        label: row.get("label"),
        version,
        revision_id,
        profile_sha256: row.get("profile_sha256"),
        profile_context_sha256,
        document,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn profile_revision_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SetupProfile, String> {
    let document_json: String = row.get("document_json");
    let document = serde_json::from_str(&document_json)
        .map_err(|error| format!("decode setup profile revision document: {error}"))?;
    Ok(SetupProfile {
        id: row.get("profile_id"),
        label: row.get("label"),
        version: row.get("version"),
        revision_id: row.get("revision_id"),
        profile_sha256: row.get("profile_sha256"),
        profile_context_sha256: row.get("profile_context_sha256"),
        document,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn validate_label<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 {
        return Err(format!("{field} must contain between 1 and 120 characters"));
    }
    Ok(value)
}

fn validate_short_identity<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 || value.chars().any(char::is_control) {
        return Err(format!("{field} must contain 1-120 printable characters"));
    }
    Ok(value)
}

fn finite_triplet(value: Option<&Value>, field: &str) -> Result<[f64; 3], String> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field} must be an array of three numbers"))?;
    if values.len() != 3 {
        return Err(format!("{field} must contain exactly three numbers"));
    }
    let mut result = [0.0; 3];
    for (index, value) in values.iter().enumerate() {
        result[index] = value
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| format!("{field}[{index}] must be finite"))?;
    }
    Ok(result)
}

fn normalize_profile_document(document: &Value) -> Result<Value, String> {
    let object = document
        .as_object()
        .ok_or_else(|| "setup profile document must be a JSON object".to_string())?;
    let dimensions = finite_triplet(object.get("room_dimensions_m"), "room_dimensions_m")?;
    if dimensions.iter().any(|number| *number <= 0.0) {
        return Err("room_dimensions_m must contain positive values".to_string());
    }
    let sensor_mount_radius = match object.get("sensor_mount_radius_m") {
        None => DEFAULT_SENSOR_MOUNT_RADIUS_M,
        Some(value) => value
            .as_f64()
            .filter(|number| number.is_finite() && *number >= 0.0 && *number <= MAX_SENSOR_MOUNT_RADIUS_M)
            .ok_or_else(|| format!("sensor_mount_radius_m must be between 0 and {MAX_SENSOR_MOUNT_RADIUS_M} m"))?,
    };
    let transmitter = object
        .get("transmitter")
        .and_then(Value::as_object)
        .ok_or_else(|| "transmitter must be an object".to_string())?;
    let tx_position = finite_triplet(transmitter.get("position_m"), "transmitter.position_m")?;
    if transmitter.get("id").and_then(Value::as_str) != Some("TX") {
        return Err("transmitter must be named TX".to_string());
    }
    if !within_sensor_bounds(tx_position, dimensions, sensor_mount_radius) {
        return Err("transmitter.position_m must be inside room_dimensions_m or sensor_mount_radius_m".to_string());
    }
    let receivers = object
        .get("receivers")
        .and_then(Value::as_array)
        .ok_or_else(|| "receivers must be an array".to_string())?;
    if receivers.len() != 4 {
        return Err("receivers must contain exactly RX1 through RX4".to_string());
    }
    for (index, receiver) in receivers.iter().enumerate() {
        let receiver = receiver
            .as_object()
            .ok_or_else(|| format!("receivers[{index}] must be an object"))?;
        let expected_id = format!("RX{}", index + 1);
        if receiver.get("id").and_then(Value::as_str) != Some(expected_id.as_str()) {
            return Err(format!("receivers must be ordered and named {expected_id}"));
        }
        let position = finite_triplet(
            receiver.get("position_m"),
            &format!("receivers[{index}].position_m"),
        )?;
        if !within_sensor_bounds(position, dimensions, sensor_mount_radius) {
            return Err(format!("{expected_id}.position_m must be inside room_dimensions_m or sensor_mount_radius_m"));
        }
    }

    if let Some(value) = object.get("mmwave") {
        let mmwave = value
            .as_object()
            .ok_or_else(|| "mmwave must be an object".to_string())?;
        let sensor = mmwave
            .get("sensor")
            .and_then(Value::as_str)
            .unwrap_or(MMWAVE_SENSOR);
        if sensor != MMWAVE_SENSOR {
            return Err(format!("mmwave.sensor must be {MMWAVE_SENSOR:?}"));
        }
        let mounting_position = finite_triplet(
            mmwave.get("mounting_position_m"),
            "mmwave.mounting_position_m",
        )?;
        if !within_sensor_bounds(mounting_position, dimensions, sensor_mount_radius) {
            return Err(
                "mmwave.mounting_position_m must be inside room_dimensions_m or sensor_mount_radius_m"
                    .to_string(),
            );
        }
        let allow_exterior = match mmwave.get("allow_exterior") {
            None => true,
            Some(value) => value
                .as_bool()
                .ok_or_else(|| "mmwave.allow_exterior must be a boolean".to_string())?,
        };
        if !allow_exterior && !within_sensor_bounds(mounting_position, dimensions, 0.0) {
            return Err(
                "mmwave.mounting_position_m must be inside room_dimensions_m when mmwave.allow_exterior is false"
                    .to_string(),
            );
        }
        if let Some(revision) = mmwave.get("mounting_revision").and_then(Value::as_str) {
            validate_short_identity(revision, "mmwave.mounting_revision")?;
        }
    }

    let points = object
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| "points must be an array".to_string())?;
    if points.len() != 9 {
        return Err("points must contain exactly P01 through P09".to_string());
    }
    for (index, point) in points.iter().enumerate() {
        let point = point
            .as_object()
            .ok_or_else(|| format!("points[{index}] must be an object"))?;
        let expected_id = format!("P{:02}", index + 1);
        if point.get("id").and_then(Value::as_str) != Some(expected_id.as_str()) {
            return Err(format!("points must be ordered and named {expected_id}"));
        }
        let coordinates = finite_triplet(
            point.get("coordinates_m"),
            &format!("points[{index}].coordinates_m"),
        )?;
        if coordinates[0] < 0.0
            || coordinates[0] > dimensions[0]
            || coordinates[1] < 0.0
            || coordinates[1] > dimensions[1]
            || coordinates[2] < 0.0
            || coordinates[2] > dimensions[2]
        {
            return Err(format!("{expected_id} must be inside room_dimensions_m"));
        }
    }

    let mut normalized = document.clone();
    if let Some(object) = normalized.as_object_mut() {
        object.insert("schema_version".to_string(), json!(PROFILE_SCHEMA_VERSION));
        object.insert("profile_kind".to_string(), json!("ruview.setup-profile"));
        object.insert("mmwave_status".to_string(), json!("NOT_CONNECTED"));
    }
    Ok(normalized)
}

fn profile_hash(document: &Value) -> Result<String, String> {
    deterministic_profile_bytes(document)
        .map(|bytes| sha256_bytes(&bytes))
        .map_err(|error| format!("canonicalize setup profile: {error}"))
}

fn horizontal_outside_distance(position: [f64; 3], dimensions: [f64; 3]) -> f64 {
    let dx = if position[0] < 0.0 {
        -position[0]
    } else if position[0] > dimensions[0] {
        position[0] - dimensions[0]
    } else {
        0.0
    };
    let dz = if position[2] < 0.0 {
        -position[2]
    } else if position[2] > dimensions[2] {
        position[2] - dimensions[2]
    } else {
        0.0
    };
    dx.hypot(dz)
}

fn within_sensor_bounds(position: [f64; 3], dimensions: [f64; 3], radius: f64) -> bool {
    position[1] >= 0.0
        && position[1] <= dimensions[1]
        && horizontal_outside_distance(position, dimensions) <= radius + f64::EPSILON
}

fn deterministic_profile_bytes(document: &Value) -> Result<Vec<u8>, String> {
    super::position_artifact::deterministic_pretty_json(document)
        .map_err(|error| error.to_string())
}

fn new_profile_id() -> String {
    let counter = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "profile-{}-{}-{}",
        Utc::now().timestamp_millis(),
        std::process::id(),
        counter
    )
}

fn new_profile_revision_id(profile_id: &str, version: i64) -> String {
    format!("{profile_id}-v{version}")
}

fn phase_index(phase: &str) -> Option<usize> {
    WORKFLOW_PHASES.iter().position(|candidate| *candidate == phase)
}

fn validate_workflow_phase(phase: &str) -> Result<(), String> {
    if phase_index(phase).is_some() {
        Ok(())
    } else {
        Err(format!("unsupported workflow phase: {phase}"))
    }
}

fn validate_phase_status(status: &str) -> Result<(), String> {
    match status {
        "READY" | "RUNNING" | "PASS" | "REUSED" | "BLOCKED" | "ERROR" => Ok(()),
        _ => Err(format!("unsupported workflow phase status: {status}")),
    }
}

fn validate_workflow_artifact_kind(kind: &str) -> Result<(), String> {
    match kind {
        "capture" | "training" | "calibration" | "prediction" | "truth" | "evaluation"
        | "model" | "dataset" => Ok(()),
        _ => Err(format!("unsupported workflow artifact kind: {kind}")),
    }
}

fn validate_relative_artifact_path(value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 240
        || value.contains('\0')
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err("artifact path must be a short relative path without control separators".to_string());
    }
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("artifact path must stay relative to the experiment data directory".to_string());
    }
    Ok(path)
}

fn bounded_payload(payload: &Value) -> Result<Value, String> {
    let encoded = serde_json::to_vec(payload)
        .map_err(|error| format!("serialize workflow phase payload: {error}"))?;
    if encoded.len() > 32 * 1024 {
        return Err("workflow phase payload must be at most 32 KiB".to_string());
    }
    Ok(payload.clone())
}

fn workflow_metric(workflow: &ExperimentWorkflow, key: &str) -> Value {
    workflow
        .events
        .iter()
        .rev()
        .find_map(|event| event.payload.get("metrics").and_then(|metrics| metrics.get(key)))
        .cloned()
        .unwrap_or(Value::Null)
}

async fn insert_phase_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    run_id: &str,
    phase: &str,
    status: &str,
    payload: &Value,
) -> Result<(), String> {
    let payload_json = serde_json::to_string(payload)
        .map_err(|error| format!("serialize phase event payload: {error}"))?;
    sqlx::query(
        "INSERT INTO experiment_phase_events
         (run_id, phase, status, payload_json, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind(phase)
    .bind(status)
    .bind(payload_json)
    .bind(timestamp())
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("insert workflow phase event: {error}"))?;
    Ok(())
}

async fn migrate(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL
         )",
    )
    .execute(pool)
    .await
    .map_err(|error| format!("create experiment migration table: {error}"))?;

    let version = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("read experiment schema version: {error}"))?;

    if version < 1 {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| format!("begin experiment schema migration: {error}"))?;
        sqlx::query(
            "CREATE TABLE experiment_runs (
                 id TEXT PRIMARY KEY,
                 label TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 source TEXT NOT NULL,
                 fixture_id TEXT NOT NULL,
                 state TEXT NOT NULL,
                 phase TEXT NOT NULL,
                 execution_status TEXT NOT NULL,
                 validation_status TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 started_at TEXT,
                 finished_at TEXT,
                 error_code TEXT,
                 error_message TEXT
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create experiment runs table: {error}"))?;
        sqlx::query(
            "CREATE TABLE experiment_artifacts (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 run_id TEXT NOT NULL REFERENCES experiment_runs(id) ON DELETE CASCADE,
                 kind TEXT NOT NULL,
                 relative_path TEXT NOT NULL,
                 sha256 TEXT NOT NULL,
                 created_at TEXT NOT NULL
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create experiment artifacts table: {error}"))?;
        sqlx::query(
            "CREATE INDEX experiment_artifacts_run_id_idx
             ON experiment_artifacts(run_id)",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create experiment artifacts index: {error}"))?;
        sqlx::query(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (1, ?)",
        )
        .bind(timestamp())
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("record experiment schema migration: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit experiment schema migration: {error}"))?;
    }

    if version < 2 {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| format!("begin control center schema migration: {error}"))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS setup_profiles (
                 id TEXT PRIMARY KEY,
                 label TEXT NOT NULL,
                 version INTEGER NOT NULL,
                 profile_sha256 TEXT NOT NULL,
                 document_json TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create setup profiles table: {error}"))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS experiment_workflows (
                 run_id TEXT PRIMARY KEY REFERENCES experiment_runs(id) ON DELETE CASCADE,
                 profile_id TEXT NOT NULL REFERENCES setup_profiles(id),
                 profile_sha256 TEXT NOT NULL,
                 dataset_version TEXT NOT NULL,
                 firmware_version TEXT NOT NULL,
                 calibration_id TEXT,
                 blind_seed INTEGER NOT NULL,
                 current_phase TEXT NOT NULL,
                 current_status TEXT NOT NULL
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create experiment workflows table: {error}"))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS experiment_phase_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 run_id TEXT NOT NULL REFERENCES experiment_runs(id) ON DELETE CASCADE,
                 phase TEXT NOT NULL,
                 status TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 created_at TEXT NOT NULL
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create experiment phase events table: {error}"))?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS experiment_phase_events_run_id_idx
             ON experiment_phase_events(run_id, id)",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create experiment phase events index: {error}"))?;
        sqlx::query(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (2, ?)",
        )
        .bind(timestamp())
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("record control center schema migration: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit control center schema migration: {error}"))?;
    }

    if version < 3 {
        // Snapshot the pre-v3 current profile before adding the revision
        // table. Existing databases only have the mutable current row, so
        // this is the one historical version that can be recovered safely.
        let legacy_rows = sqlx::query(
            "SELECT id, label, version, profile_sha256, document_json, created_at
             FROM setup_profiles",
        )
        .fetch_all(pool)
        .await
        .map_err(|error| format!("read legacy setup profiles: {error}"))?;
        let mut legacy_snapshots = Vec::with_capacity(legacy_rows.len());
        for row in legacy_rows {
            let document_json: String = row.get("document_json");
            let document: Value = serde_json::from_str(&document_json)
                .map_err(|error| format!("decode legacy setup profile: {error}"))?;
            let profile_id: String = row.get("id");
            let version: i64 = row.get("version");
            legacy_snapshots.push((
                profile_id.clone(),
                version,
                new_profile_revision_id(&profile_id, version),
                row.get::<String, _>("label"),
                row.get::<String, _>("profile_sha256"),
                profile_context_sha256(&document)?,
                document_json,
                row.get::<String, _>("created_at"),
            ));
        }

        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| format!("begin calibration persistence migration: {error}"))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS setup_profile_revisions (
                 revision_id TEXT PRIMARY KEY,
                 profile_id TEXT NOT NULL REFERENCES setup_profiles(id) ON DELETE CASCADE,
                 version INTEGER NOT NULL,
                 label TEXT NOT NULL,
                 profile_sha256 TEXT NOT NULL,
                 profile_context_sha256 TEXT NOT NULL,
                 document_json TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 UNIQUE(profile_id, version)
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create setup profile revisions table: {error}"))?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS setup_profile_revisions_profile_idx
             ON setup_profile_revisions(profile_id, version)",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create setup profile revisions index: {error}"))?;

        // SQLite permits adding nullable columns to the existing workflow
        // table. New runs always populate them; old runs remain readable.
        for statement in [
            "ALTER TABLE experiment_workflows ADD COLUMN profile_revision_id TEXT",
            "ALTER TABLE experiment_workflows ADD COLUMN profile_context_sha256 TEXT",
            "ALTER TABLE experiment_workflows ADD COLUMN calibration_source TEXT",
            "ALTER TABLE experiment_workflows ADD COLUMN calibration_context_sha256 TEXT",
        ] {
            sqlx::query(statement)
                .execute(&mut *transaction)
                .await
                .map_err(|error| format!("extend experiment workflow metadata: {error}"))?;
        }
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS calibration_bundles (
                 calibration_id TEXT PRIMARY KEY,
                 profile_id TEXT NOT NULL REFERENCES setup_profiles(id),
                 profile_revision_id TEXT NOT NULL,
                 profile_sha256 TEXT NOT NULL,
                 profile_context_sha256 TEXT NOT NULL,
                 setup_id TEXT NOT NULL,
                 setup_sha256 TEXT NOT NULL,
                 calibration_context_sha256 TEXT NOT NULL,
                 algorithm_version TEXT NOT NULL,
                 relative_path TEXT NOT NULL UNIQUE,
                 sha256 TEXT NOT NULL,
                 captured_at TEXT NOT NULL,
                 status TEXT NOT NULL
             )",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create calibration bundles table: {error}"))?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS calibration_bundles_context_idx
             ON calibration_bundles(profile_id, profile_context_sha256, setup_id, setup_sha256, captured_at)",
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| format!("create calibration bundles index: {error}"))?;

        for (
            profile_id,
            version,
            revision_id,
            label,
            profile_sha256,
            profile_context_sha256,
            document_json,
            created_at,
        ) in legacy_snapshots
        {
            sqlx::query(
                "INSERT OR IGNORE INTO setup_profile_revisions
                 (revision_id, profile_id, version, label, profile_sha256,
                  profile_context_sha256, document_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&revision_id)
            .bind(&profile_id)
            .bind(version)
            .bind(label)
            .bind(profile_sha256)
            .bind(&profile_context_sha256)
            .bind(document_json)
            .bind(created_at)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("backfill setup profile revision: {error}"))?;
            sqlx::query(
                "UPDATE experiment_workflows
                 SET profile_revision_id = ?, profile_context_sha256 = ?
                 WHERE profile_id = ? AND profile_revision_id IS NULL",
            )
            .bind(&revision_id)
            .bind(&profile_context_sha256)
            .bind(&profile_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("backfill workflow profile context: {error}"))?;
        }

        sqlx::query("INSERT INTO schema_migrations(version, applied_at) VALUES (3, ?)")
            .bind(timestamp())
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("record calibration persistence migration: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("commit calibration persistence migration: {error}"))?;
    }

    Ok(())
}

fn new_run_id() -> String {
    let counter = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "run-{}-{}-{}",
        Utc::now().timestamp_millis(),
        std::process::id(),
        counter
    )
}

fn timestamp() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_calibration_bundle(
        profile: &SetupProfile,
        setup_id: &str,
        setup_sha256: &str,
    ) -> CalibrationBundle {
        let context_sha256 = crate::calibration_persistence::calibration_context_sha256(
            &profile.profile_context_sha256,
            setup_id,
            setup_sha256,
        )
        .expect("test calibration context hash");
        let context = crate::calibration_persistence::CalibrationContext {
            profile_id: profile.id.clone(),
            profile_revision_id: profile.revision_id.clone(),
            profile_sha256: profile.profile_sha256.clone(),
            profile_context_sha256: profile.profile_context_sha256.clone(),
            setup_id: setup_id.to_string(),
            setup_sha256: setup_sha256.to_string(),
            calibration_context_sha256: context_sha256,
        };

        let mut fingerprint = crate::d6_fingerprint::NodeFingerprintState::default();
        fingerprint
            .install_reference_for_test(&[1.0, 2.0, 1.0, 2.0])
            .expect("test D6 reference");
        let d6_reference = fingerprint
            .reference_for_test()
            .expect("test D6 reference installed");
        let d5_reference = crate::d5_presence::PresenceReference {
            median: 0.02,
            mad: 0.0,
            scale: crate::d5_presence::ROBUST_SCALE_FLOOR,
            block_count: crate::d5_presence::MIN_CALIBRATION_BLOCKS,
            sample_count: crate::d5_presence::MIN_CALIBRATION_BLOCKS
                * crate::d5_presence::MIN_CALIBRATION_SAMPLES_PER_BLOCK,
        };
        let nodes = (1..=3)
            .map(|node_id| crate::calibration_persistence::CalibrationNodeBundle {
                node_id,
                d5: Some(d5_reference),
                d6: Some(d6_reference.clone()),
            })
            .collect();
        CalibrationBundle::new(
            "calibration-test-1".to_string(),
            &context,
            "2026-08-28T12:00:00Z".to_string(),
            nodes,
        )
        .expect("valid test calibration bundle")
    }

    fn valid_profile() -> Value {
        json!({
            "room_dimensions_m": [4.02, 2.59, 3.44],
            "transmitter": {"id": "TX", "position_m": [1.51, 1.19, 0.39]},
            "receivers": [
                {"id": "RX1", "role": "receiver", "position_m": [0.0, 0.5, 0.28]},
                {"id": "RX2", "role": "receiver", "position_m": [4.02, 0.87, 0.97]},
                {"id": "RX3", "role": "receiver", "position_m": [0.0, 0.74, 2.11]},
                {"id": "RX4", "role": "receiver", "position_m": [4.02, 0.87, 2.46]}
            ],
            "points": [
                {"id": "P01", "coordinates_m": [1.01, 0.0, 0.86]},
                {"id": "P02", "coordinates_m": [2.01, 0.0, 0.86]},
                {"id": "P03", "coordinates_m": [3.01, 0.0, 0.86]},
                {"id": "P04", "coordinates_m": [1.01, 0.0, 1.72]},
                {"id": "P05", "coordinates_m": [2.01, 0.0, 1.72]},
                {"id": "P06", "coordinates_m": [3.01, 0.0, 1.72]},
                {"id": "P07", "coordinates_m": [1.01, 0.0, 2.58]},
                {"id": "P08", "coordinates_m": [2.01, 0.0, 2.58]},
                {"id": "P09", "coordinates_m": [3.01, 0.0, 2.58]}
            ]
        })
    }

    #[tokio::test]
    async fn sqlite_catalog_survives_reopen_and_replay_registers_hashed_report() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        assert_eq!(store.run_count().await.expect("count runs"), 0);

        let created = store
            .create_run("Synthetic smoke run", SUPPORTED_FIXTURE_ID)
            .await
            .expect("create run");
        assert_eq!(created.state, "created");
        assert_eq!(created.validation_status, "UNVALIDATED");

        let completed = replay_run(&store, &created.id).await.expect("replay run");
        assert_eq!(completed.state, "completed");
        assert_eq!(completed.execution_status, "PASS");
        assert_eq!(completed.validation_status, "UNVALIDATED");
        assert_eq!(completed.artifacts.len(), 1);
        assert_eq!(completed.artifacts[0].kind, "report");

        let report_path = directory.path().join(&completed.artifacts[0].relative_path);
        let report = fs::read(&report_path).await.expect("read report");
        assert_eq!(sha256_bytes(&report), completed.artifacts[0].sha256);
        let report_json: Value = serde_json::from_slice(&report).expect("parse report");
        assert_eq!(report_json["live_position_approved"], false);
        assert_eq!(report_json["validation_status"], "UNVALIDATED");

        let reopened = ExperimentStore::open(directory.path())
            .await
            .expect("reopen experiment store");
        let loaded = reopened
            .get_run(&created.id)
            .await
            .expect("load persisted run")
            .expect("persisted run exists");
        assert_eq!(loaded.artifacts[0].sha256, completed.artifacts[0].sha256);
        assert!(replay_run(&reopened, &created.id).await.is_err());
    }

    #[tokio::test]
    async fn unsupported_fixture_is_rejected_before_persistence() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let error = store
            .create_run("bad", "unknown-fixture")
            .await
            .expect_err("unknown fixture must fail");
        assert!(error.contains("unsupported fixture_id"));
        assert_eq!(store.run_count().await.expect("count runs"), 0);
    }

    #[tokio::test]
    async fn wifi_workflow_persists_profiles_phases_and_unvalidated_report() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");

        let profile = store
            .create_profile("Fixed room", &valid_profile())
            .await
            .expect("create setup profile");
        assert_eq!(profile.version, 1);
        assert_eq!(profile.document["schema_version"], PROFILE_SCHEMA_VERSION);
        assert_eq!(profile.document["mmwave_status"], "NOT_CONNECTED");
        assert_eq!(profile.profile_sha256.len(), 64);

        let mut changed_document = valid_profile();
        changed_document["room_dimensions_m"][0] = json!(4.12);
        let updated = store
            .update_profile(&profile.id, "Fixed room v2", &changed_document)
            .await
            .expect("update setup profile");
        assert_eq!(updated.version, 2);
        assert_ne!(updated.profile_sha256, profile.profile_sha256);

        let run = store
            .create_workflow_run(
                "WiFi position run",
                &updated.id,
                Some(&updated.revision_id),
                "dataset-v1",
                "firmware-v1",
                42,
            )
            .await
            .expect("create WiFi workflow");
        assert_eq!(run.workflow.as_ref().expect("workflow metadata").current_phase, "create_experiment");
        assert_eq!(run.workflow.as_ref().expect("workflow metadata").events.len(), 1);

        fs::write(directory.path().join("prediction.json"), b"{\"predictions\":[]}")
            .await
            .expect("write prediction artifact");
        let registered = store
            .register_workflow_artifact(&run.id, "prediction", "prediction.json")
            .await
            .expect("register prediction artifact");
        assert_eq!(registered.artifacts.len(), 1);
        assert_eq!(registered.artifacts[0].kind, "prediction");
        assert!(store
            .register_workflow_artifact(&run.id, "truth", "../outside.json")
            .await
            .is_err());

        let initial_error = store
            .advance_workflow(&run.id, "report", "PASS", &json!({}))
            .await
            .expect_err("workflow must not skip phases");
        assert!(initial_error.contains("workflow phase must stay"));

        let mut current = run;
        for phase in WORKFLOW_PHASES {
            if phase == "create_experiment" {
                current = store
                    .advance_workflow(&current.id, phase, "PASS", &json!({}))
                    .await
                    .expect("complete create phase");
                continue;
            }
            let payload = if phase == "evaluate" {
                json!({"metrics": {"accuracy": 0.0, "coverage": "not_recorded"}})
            } else {
                json!({"software_only": true})
            };
            current = store
                .advance_workflow(&current.id, phase, "PASS", &payload)
                .await
                .expect("complete workflow phase");
        }

        let report_run = store
            .write_workflow_report(&current.id)
            .await
            .expect("write workflow report");
        assert_eq!(report_run.state, "completed");
        assert_eq!(report_run.validation_status, "UNVALIDATED");
        assert_eq!(report_run.artifacts.len(), 2);
        let report = store
            .report_json(&current.id)
            .await
            .expect("load workflow report")
            .expect("workflow report exists");
        assert_eq!(report["live_position_approved"], false);
        assert_eq!(report["mmwave_evidence"], false);
        assert_eq!(report["metrics"]["accuracy"], 0.0);
        assert_eq!(report["artifacts"][0]["kind"], "prediction");

        let reopened = ExperimentStore::open(directory.path())
            .await
            .expect("reopen experiment store");
        let profiles = reopened.list_profiles().await.expect("list profiles");
        assert_eq!(profiles.len(), 1);
        let loaded = reopened
            .get_run(&current.id)
            .await
            .expect("load workflow")
            .expect("workflow persists");
        assert_eq!(loaded.workflow.expect("workflow metadata").events.len(), 11);
    }

    #[tokio::test]
    async fn setup_profile_rejects_nodes_outside_room() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let mut document = valid_profile();
        document["sensor_mount_radius_m"] = json!(0.0);
        document["receivers"][0]["position_m"][0] = json!(-0.01);
        let error = store
            .create_profile("invalid room", &document)
            .await
            .expect_err("outside receiver must fail");
        assert!(error.contains("RX1.position_m"));
    }

    #[tokio::test]
    async fn setup_profile_accepts_nodes_within_sensor_mount_radius() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let mut document = valid_profile();
        document["sensor_mount_radius_m"] = json!(0.5);
        document["transmitter"]["position_m"][0] = json!(-0.25);
        document["receivers"][1]["position_m"][0] = json!(4.27);
        let profile = store
            .create_profile("exterior sensor mounts", &document)
            .await
            .expect("nodes inside configured exterior radius must pass");
        assert_eq!(profile.document["sensor_mount_radius_m"], 0.5);
    }

    #[tokio::test]
    async fn setup_profile_persists_mmwave_mounting_position_and_hashes_it() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let mut document = valid_profile();
        document["mmwave"] = json!({
            "sensor": "HLK-LD2450",
            "mounting_position_m": [-0.25, 1.2, 1.72],
            "mounting_revision": "breadboard-v1"
        });

        let profile = store
            .create_profile("mmWave mount", &document)
            .await
            .expect("mmWave mounting position must be accepted");
        assert_eq!(profile.document["mmwave"]["sensor"], "HLK-LD2450");
        assert_eq!(profile.document["mmwave"]["mounting_position_m"][0], -0.25);
        assert_eq!(profile.document["mmwave"]["mounting_revision"], "breadboard-v1");
        assert_eq!(profile.profile_sha256.len(), 64);

        let mut changed = document;
        changed["mmwave"]["mounting_position_m"][0] = json!(-0.20);
        let updated = store
            .update_profile(&profile.id, "mmWave mount v2", &changed)
            .await
            .expect("updated mmWave mounting position must be accepted");
        assert_ne!(updated.profile_sha256, profile.profile_sha256);
        assert_eq!(updated.document["mmwave"]["mounting_position_m"][0], -0.20);
    }

    #[tokio::test]
    async fn setup_profile_rejects_mmwave_mount_outside_sensor_mount_radius() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let mut document = valid_profile();
        document["sensor_mount_radius_m"] = json!(0.5);
        document["mmwave"] = json!({
            "sensor": "HLK-LD2450",
            "mounting_position_m": [-0.51, 1.2, 1.72]
        });

        let error = store
            .create_profile("invalid mmWave mount", &document)
            .await
            .expect_err("mmWave mount outside the configured radius must fail");
        assert!(error.contains("mmwave.mounting_position_m"));
    }

    #[tokio::test]
    async fn setup_profile_rejects_mmwave_exterior_mount_when_interior_only() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let mut document = valid_profile();
        document["sensor_mount_radius_m"] = json!(2.0);
        document["mmwave"] = json!({
            "sensor": "HLK-LD2450",
            "mounting_position_m": [-0.25, 1.2, 1.72],
            "allow_exterior": false
        });

        let error = store
            .create_profile("interior-only mmWave mount", &document)
            .await
            .expect_err("interior-only mmWave mount must reject exterior coordinates");
        assert!(error.contains("mmwave.allow_exterior is false"));
    }

    #[tokio::test]
    async fn setup_profile_accepts_interior_mmwave_mount_when_interior_only() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let mut document = valid_profile();
        document["transmitter"]["position_m"][0] = json!(-0.25);
        document["mmwave"] = json!({
            "sensor": "HLK-LD2450",
            "mounting_position_m": [0.0, 1.2, 1.72],
            "allow_exterior": false
        });

        let profile = store
            .create_profile("interior-only mmWave mount", &document)
            .await
            .expect("interior mmWave mount must pass");
        assert_eq!(profile.document["mmwave"]["allow_exterior"], false);
    }

    #[tokio::test]
    async fn setup_profile_revisions_are_immutable_and_context_bound() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let profile = store
            .create_profile("Fixed room", &valid_profile())
            .await
            .expect("create setup profile");
        let original_revision = store
            .get_profile_revision(&profile.id, &profile.revision_id)
            .await
            .expect("load original profile revision")
            .expect("original profile revision exists");

        let mut point_edit = profile.document.clone();
        point_edit["points"][0]["coordinates_m"][0] = json!(1.10);
        let updated = store
            .update_profile(&profile.id, "Fixed room with updated labels", &point_edit)
            .await
            .expect("save new profile version");
        assert_eq!(updated.version, 2);
        assert_ne!(updated.revision_id, profile.revision_id);
        assert_ne!(updated.profile_sha256, profile.profile_sha256);
        assert_eq!(updated.profile_context_sha256, profile.profile_context_sha256);
        assert_eq!(original_revision.version, 1);
        assert_eq!(original_revision.document, profile.document);

        let no_op = store
            .update_profile(&updated.id, &updated.label, &updated.document)
            .await
            .expect("repeat identical profile save");
        assert_eq!(no_op.version, updated.version);
        assert_eq!(no_op.revision_id, updated.revision_id);

        let mut moved_receiver = updated.document.clone();
        moved_receiver["receivers"][0]["position_m"][0] = json!(0.20);
        let moved = store
            .update_profile(&updated.id, "Fixed room with moved RX1", &moved_receiver)
            .await
            .expect("save moved receiver profile version");
        assert_eq!(moved.version, 3);
        assert_ne!(moved.profile_context_sha256, updated.profile_context_sha256);
        assert_eq!(
            store
                .get_profile_revision(&updated.id, &updated.revision_id)
                .await
                .expect("load version 2")
                .expect("version 2 exists")
                .document,
            updated.document
        );
    }

    #[tokio::test]
    async fn v2_catalog_migration_preserves_profile_and_workflow_binding() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let db_path = directory.path().join("observatory-experiments.sqlite3");
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("create legacy SQLite catalog");

        sqlx::query(
            "CREATE TABLE schema_migrations (
                 version INTEGER PRIMARY KEY,
                 applied_at TEXT NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .expect("create legacy migration table");
        sqlx::query(
            "CREATE TABLE experiment_runs (
                 id TEXT PRIMARY KEY,
                 label TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 source TEXT NOT NULL,
                 fixture_id TEXT NOT NULL,
                 state TEXT NOT NULL,
                 phase TEXT NOT NULL,
                 execution_status TEXT NOT NULL,
                 validation_status TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 started_at TEXT,
                 finished_at TEXT,
                 error_code TEXT,
                 error_message TEXT
             )",
        )
        .execute(&pool)
        .await
        .expect("create legacy runs table");
        sqlx::query(
            "CREATE TABLE experiment_artifacts (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 run_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 relative_path TEXT NOT NULL,
                 sha256 TEXT NOT NULL,
                 created_at TEXT NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .expect("create legacy artifacts table");
        sqlx::query(
            "CREATE TABLE setup_profiles (
                 id TEXT PRIMARY KEY,
                 label TEXT NOT NULL,
                 version INTEGER NOT NULL,
                 profile_sha256 TEXT NOT NULL,
                 document_json TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .expect("create legacy profiles table");
        sqlx::query(
            "CREATE TABLE experiment_workflows (
                 run_id TEXT PRIMARY KEY,
                 profile_id TEXT NOT NULL,
                 profile_sha256 TEXT NOT NULL,
                 dataset_version TEXT NOT NULL,
                 firmware_version TEXT NOT NULL,
                 calibration_id TEXT,
                 blind_seed INTEGER NOT NULL,
                 current_phase TEXT NOT NULL,
                 current_status TEXT NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .expect("create legacy workflows table");
        sqlx::query(
            "CREATE TABLE experiment_phase_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 run_id TEXT NOT NULL,
                 phase TEXT NOT NULL,
                 status TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 created_at TEXT NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .expect("create legacy phase events table");

        let document = valid_profile();
        let document_json = serde_json::to_string(&document).expect("serialize legacy profile");
        let profile_sha256 = profile_hash(&document).expect("hash legacy profile");
        sqlx::query(
            "INSERT INTO setup_profiles
             (id, label, version, profile_sha256, document_json, created_at, updated_at)
             VALUES ('profile-legacy', 'Legacy room', 4, ?, ?, '2026-08-01T00:00:00Z', '2026-08-02T00:00:00Z')",
        )
        .bind(&profile_sha256)
        .bind(&document_json)
        .execute(&pool)
        .await
        .expect("insert legacy profile");
        sqlx::query(
            "INSERT INTO experiment_runs
             (id, label, kind, source, fixture_id, state, phase,
              execution_status, validation_status, created_at)
             VALUES ('run-legacy', 'Legacy run', ?, ?, 'none', 'running',
                     'seal_setup', 'RUNNING', 'UNVALIDATED', '2026-08-02T00:00:00Z')",
        )
        .bind(WORKFLOW_KIND)
        .bind(WORKFLOW_SOURCE)
        .execute(&pool)
        .await
        .expect("insert legacy run");
        sqlx::query(
            "INSERT INTO experiment_workflows
             (run_id, profile_id, profile_sha256, dataset_version,
              firmware_version, calibration_id, blind_seed, current_phase, current_status)
             VALUES ('run-legacy', 'profile-legacy', ?, 'dataset-v1',
                     'firmware-v1', NULL, 5, 'seal_setup', 'PASS')",
        )
        .bind(&profile_sha256)
        .execute(&pool)
        .await
        .expect("insert legacy workflow");
        sqlx::query(
            "INSERT INTO schema_migrations(version, applied_at)
             VALUES (1, '2026-08-01T00:00:00Z'), (2, '2026-08-01T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .expect("mark legacy schema versions");
        pool.close().await;

        let store = ExperimentStore::open(directory.path())
            .await
            .expect("migrate legacy SQLite catalog");
        let profile = store
            .get_profile("profile-legacy")
            .await
            .expect("load migrated profile")
            .expect("migrated profile exists");
        assert_eq!(profile.version, 4);
        assert_eq!(profile.revision_id, "profile-legacy-v4");
        assert_eq!(profile.document, document);
        assert_eq!(profile.profile_sha256, profile_sha256);

        let run = store
            .get_run("run-legacy")
            .await
            .expect("load migrated run")
            .expect("migrated run exists");
        let workflow = run.workflow.expect("migrated workflow metadata");
        assert_eq!(workflow.profile_revision_id.as_deref(), Some("profile-legacy-v4"));
        assert_eq!(
            workflow.profile_context_sha256.as_deref(),
            Some(profile.profile_context_sha256.as_str())
        );

        let version: i64 = sqlx::query_scalar(
            "SELECT MAX(version) FROM schema_migrations",
        )
        .fetch_one(&store.pool)
        .await
        .expect("read migrated schema version");
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn stored_calibration_reuses_unchanged_geometry_and_rejects_moved_receiver() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let profile = store
            .create_profile("Fixed room", &valid_profile())
            .await
            .expect("create setup profile");
        let setup_id = "setup-test-1";
        let setup_sha256 = "a".repeat(64);
        let bundle = test_calibration_bundle(&profile, setup_id, &setup_sha256);
        let summary = store
            .persist_calibration_bundle(&bundle)
            .await
            .expect("persist calibration bundle");
        assert_eq!(summary.calibration_id, bundle.calibration_id);
        assert!(directory
            .path()
            .join("calibrations/calibration-test-1.json")
            .is_file());

        let compatible = store
            .find_compatible_calibration(
                &profile.id,
                &profile.profile_context_sha256,
                setup_id,
                setup_sha256.as_str(),
            )
            .await
            .expect("find compatible calibration")
            .expect("compatible calibration exists");
        assert_eq!(compatible.calibration_id, bundle.calibration_id);

        let mut point_edit = profile.document.clone();
        point_edit["points"][0]["coordinates_m"][0] = json!(1.10);
        let cosmetic_update = store
            .update_profile(&profile.id, "Fixed room with point labels", &point_edit)
            .await
            .expect("save non-baseline profile edit");
        assert_eq!(
            cosmetic_update.profile_context_sha256,
            profile.profile_context_sha256
        );
        assert!(store
            .find_compatible_calibration(
                &cosmetic_update.id,
                &cosmetic_update.profile_context_sha256,
                setup_id,
                setup_sha256.as_str(),
            )
            .await
            .expect("find calibration after non-baseline edit")
            .is_some());

        let mut moved_receiver = cosmetic_update.document.clone();
        moved_receiver["receivers"][0]["position_m"][0] = json!(0.20);
        let moved = store
            .update_profile(&cosmetic_update.id, "Fixed room with moved RX1", &moved_receiver)
            .await
            .expect("save moved receiver version");
        assert_ne!(moved.profile_context_sha256, cosmetic_update.profile_context_sha256);
        assert!(store
            .find_compatible_calibration(
                &moved.id,
                &moved.profile_context_sha256,
                setup_id,
                setup_sha256.as_str(),
            )
            .await
            .expect("find calibration after moved receiver")
            .is_none());
    }

    #[tokio::test]
    async fn reused_calibration_is_a_valid_terminal_workflow_phase() {
        let directory = tempfile::tempdir().expect("temporary experiment directory");
        let store = ExperimentStore::open(directory.path())
            .await
            .expect("open experiment store");
        let profile = store
            .create_profile("Fixed room", &valid_profile())
            .await
            .expect("create setup profile");
        let run = store
            .create_workflow_run(
                "Reuse calibration run",
                &profile.id,
                Some(&profile.revision_id),
                "dataset-v1",
                "firmware-v1",
                7,
            )
            .await
            .expect("create workflow");
        let run = store
            .advance_workflow(&run.id, "create_experiment", "PASS", &json!({}))
            .await
            .expect("complete create phase");
        let run = store
            .advance_workflow(&run.id, "seal_setup", "PASS", &json!({}))
            .await
            .expect("complete seal phase");
        let run = store
            .advance_workflow(
                &run.id,
                "empty_calibration",
                "REUSED",
                &json!({
                    "calibration_id": "calibration-test-1",
                    "calibration_source": "reused",
                    "calibration_context_sha256": "b".repeat(64),
                }),
            )
            .await
            .expect("mark empty calibration reused");
        let workflow = run.workflow.as_ref().expect("workflow");
        assert_eq!(workflow.current_phase, "empty_calibration");
        assert_eq!(workflow.current_status, "REUSED");
        assert_eq!(workflow.calibration_source.as_deref(), Some("reused"));
        assert_eq!(workflow.calibration_id.as_deref(), Some("calibration-test-1"));
    }
}
