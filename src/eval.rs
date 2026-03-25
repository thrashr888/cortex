use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::Memory;
use crate::{context, db};

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Benchmark {
    pub name: String,
    pub cases: Vec<BenchmarkCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCase {
    pub name: String,
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub project_memories: Vec<BenchmarkMemory>,
    #[serde(default)]
    pub global_memories: Vec<BenchmarkMemory>,
    #[serde(default)]
    pub expected_project_keys: Vec<String>,
    #[serde(default)]
    pub expected_global_keys: Vec<String>,
    #[serde(default)]
    pub disallowed_project_keys: Vec<String>,
    #[serde(default)]
    pub disallowed_global_keys: Vec<String>,
    #[serde(default)]
    pub required_context_substrings: Vec<String>,
    #[serde(default)]
    pub forbidden_context_substrings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMemory {
    pub key: String,
    pub content: String,
    #[serde(rename = "type")]
    pub memory_type: String,
    #[serde(default = "default_memory_confidence")]
    pub confidence: f64,
    #[serde(default)]
    pub entities: Vec<BenchmarkEntity>,
    #[serde(default)]
    pub relationships: Vec<BenchmarkRelationship>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkEntity {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRelationship {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub relation_type: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub benchmark: String,
    pub total_score: f64,
    pub recall_score: f64,
    pub context_score: f64,
    pub hillclimb_score: f64,
    pub case_reports: Vec<CaseReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseReport {
    pub name: String,
    pub score: f64,
    pub recall_score: f64,
    pub context_score: f64,
    pub hillclimb_score: f64,
    pub retrieved_project_keys: Vec<String>,
    pub retrieved_global_keys: Vec<String>,
    pub missing_required_context: Vec<String>,
    pub present_forbidden_context: Vec<String>,
}

#[derive(Debug)]
struct RetrievalOutcome {
    project_keys: Vec<String>,
    global_keys: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct RecallExpectations<'a> {
    expected_project: &'a [String],
    expected_global: &'a [String],
    disallowed_project: &'a [String],
    disallowed_global: &'a [String],
}

const fn default_limit() -> usize {
    5
}
const fn default_confidence() -> f64 {
    0.8
}
const fn default_memory_confidence() -> f64 {
    1.0
}

pub fn run_benchmark_file(path: &Path) -> Result<BenchmarkReport> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read benchmark fixture at {}", path.display()))?;
    let benchmark: Benchmark = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse benchmark fixture at {}", path.display()))?;
    run_benchmark(&benchmark)
}

pub fn format_report(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("benchmark: {}\n", report.benchmark));
    out.push_str(&format!("total_score: {:.2}\n", report.total_score));
    out.push_str(&format!("recall_score: {:.2}\n", report.recall_score));
    out.push_str(&format!("context_score: {:.2}\n", report.context_score));
    out.push_str(&format!("hillclimb_score: {:.2}\n", report.hillclimb_score));
    out.push('\n');

    for case in &report.case_reports {
        out.push_str(&format!("- {}\n", case.name));
        out.push_str(&format!("  score: {:.2}\n", case.score));
        out.push_str(&format!("  recall: {:.2}\n", case.recall_score));
        out.push_str(&format!("  context: {:.2}\n", case.context_score));
        out.push_str(&format!("  hillclimb: {:.2}\n", case.hillclimb_score));
        if !case.retrieved_project_keys.is_empty() {
            out.push_str(&format!(
                "  project_hits: {}\n",
                case.retrieved_project_keys.join(", ")
            ));
        }
        if !case.retrieved_global_keys.is_empty() {
            out.push_str(&format!(
                "  global_hits: {}\n",
                case.retrieved_global_keys.join(", ")
            ));
        }
        if !case.missing_required_context.is_empty() {
            out.push_str(&format!(
                "  missing_context: {}\n",
                case.missing_required_context.join(" | ")
            ));
        }
        if !case.present_forbidden_context.is_empty() {
            out.push_str(&format!(
                "  forbidden_context: {}\n",
                case.present_forbidden_context.join(" | ")
            ));
        }
    }

    out
}

fn run_benchmark(benchmark: &Benchmark) -> Result<BenchmarkReport> {
    let mut case_reports = Vec::new();
    for case in &benchmark.cases {
        case_reports.push(run_case(case)?);
    }

    let total_score = average(case_reports.iter().map(|c| c.score));
    let recall_score = average(case_reports.iter().map(|c| c.recall_score));
    let context_score = average(case_reports.iter().map(|c| c.context_score));
    let hillclimb_score = round2(case_reports.iter().map(|c| c.hillclimb_score).sum());

    Ok(BenchmarkReport {
        benchmark: benchmark.name.clone(),
        total_score,
        recall_score,
        context_score,
        hillclimb_score,
        case_reports,
    })
}

fn run_case(case: &BenchmarkCase) -> Result<CaseReport> {
    let scratch = ScratchSpace::new()?;
    let raw_conn = db::open_raw_db(&scratch.project_dir.join("raw.db"))?;
    let cons_conn = db::open_consolidated_db(&scratch.project_dir.join("consolidated.db"))?;
    let global_cons = db::open_consolidated_db(&scratch.global_dir.join("consolidated.db"))?;

    let mut project_lookup = HashMap::new();
    let mut global_lookup = HashMap::new();

    seed_memories(&raw_conn, &cons_conn, &case.project_memories, &mut project_lookup)?;
    seed_global_memories(&global_cons, &case.global_memories, &mut global_lookup)?;

    let retrieved = retrieve_keys(&raw_conn, Some(&global_cons), &project_lookup, &global_lookup, &case.query, case.limit)?;
    let recall_score = score_recall(
        &retrieved.project_keys,
        &retrieved.global_keys,
        RecallExpectations {
            expected_project: &case.expected_project_keys,
            expected_global: &case.expected_global_keys,
            disallowed_project: &case.disallowed_project_keys,
            disallowed_global: &case.disallowed_global_keys,
        },
    );

    let rendered_context = context::format_context(
        &cons_conn,
        &raw_conn,
        Some(&global_cons),
        false,
        Some(&case.query),
        case.limit,
    )?;
    let (context_score, missing_required_context, present_forbidden_context) =
        score_context(&rendered_context, &case.required_context_substrings, &case.forbidden_context_substrings);
    let hillclimb_score = score_hillclimb_case(
        &retrieved.project_keys,
        &retrieved.global_keys,
        &rendered_context,
        RecallExpectations {
            expected_project: &case.expected_project_keys,
            expected_global: &case.expected_global_keys,
            disallowed_project: &case.disallowed_project_keys,
            disallowed_global: &case.disallowed_global_keys,
        },
        &case.required_context_substrings,
        &case.forbidden_context_substrings,
    );

    let score = round2((recall_score * 0.65) + (context_score * 0.35));

    Ok(CaseReport {
        name: case.name.clone(),
        score,
        recall_score,
        context_score,
        hillclimb_score,
        retrieved_project_keys: retrieved.project_keys,
        retrieved_global_keys: retrieved.global_keys,
        missing_required_context,
        present_forbidden_context,
    })
}

fn seed_memories(
    raw_conn: &rusqlite::Connection,
    cons_conn: &rusqlite::Connection,
    memories: &[BenchmarkMemory],
    lookup: &mut HashMap<i64, String>,
) -> Result<()> {
    for memory in memories {
        let raw_id = db::save_memory(raw_conn, &memory.content, &memory.memory_type, "eval")?;
        db::update_memory_importance(raw_conn, raw_id, memory.confidence)?;
        let mut entity_ids = Vec::new();
        let mut entity_name_to_id = HashMap::new();
        for entity in &memory.entities {
            let entity_id = db::upsert_entity(
                raw_conn,
                &entity.name,
                &entity.entity_type,
                entity.description.as_deref(),
            )?;
            entity_ids.push(entity_id);
            entity_name_to_id.insert(entity.name.clone(), entity_id);
        }
        if !entity_ids.is_empty() {
            db::update_memory_entities(raw_conn, raw_id, &entity_ids)?;
        }
        for relationship in &memory.relationships {
            if let (Some(source_id), Some(target_id)) = (
                entity_name_to_id.get(&relationship.source),
                entity_name_to_id.get(&relationship.target),
            ) {
                db::upsert_relationship(
                    raw_conn,
                    *source_id,
                    *target_id,
                    &relationship.relation_type,
                    raw_id,
                    relationship.confidence,
                )?;
            }
        }
        db::insert_consolidated(
            cons_conn,
            &memory.content,
            &memory.memory_type,
            &[raw_id],
            memory.confidence,
        )?;
        lookup.insert(raw_id, memory.key.clone());
    }
    Ok(())
}

fn seed_global_memories(
    global_cons: &rusqlite::Connection,
    memories: &[BenchmarkMemory],
    lookup: &mut HashMap<i64, String>,
) -> Result<()> {
    for memory in memories {
        let id = db::insert_consolidated(
            global_cons,
            &memory.content,
            &memory.memory_type,
            &[],
            memory.confidence,
        )?;
        lookup.insert(id, memory.key.clone());
    }
    Ok(())
}

fn retrieve_keys(
    raw_conn: &rusqlite::Connection,
    global_cons: Option<&rusqlite::Connection>,
    project_lookup: &HashMap<i64, String>,
    global_lookup: &HashMap<i64, String>,
    query: &str,
    limit: usize,
) -> Result<RetrievalOutcome> {
    let mut project_memories = db::recall_by_entity(raw_conn, query, true, limit)?;
    if project_memories.is_empty() {
        project_memories = db::recall_memories(raw_conn, query, limit)?;
    }

    let mut global_memories = match global_cons {
        Some(conn) => db::search_consolidated(conn, query, limit)?,
        None => Vec::new(),
    };

    match context::scope_decision_for_query(
        project_memories
            .first()
            .map(|memory: &Memory| (memory.content.as_str(), memory.r#type.as_str())),
        global_memories
            .first()
            .map(|memory| (memory.content.as_str(), memory.r#type.as_str())),
        query,
    ) {
        context::ScopeDecision::ProjectOnly => global_memories.clear(),
        context::ScopeDecision::GlobalOnly => project_memories.clear(),
        context::ScopeDecision::KeepBoth => {}
    }

    let project_keys = project_memories
        .iter()
        .filter_map(|m| project_lookup.get(&m.id).cloned())
        .collect();
    let global_keys = global_memories
        .into_iter()
        .filter_map(|m| global_lookup.get(&m.id).cloned())
        .collect();

    Ok(RetrievalOutcome {
        project_keys,
        global_keys,
    })
}

fn score_recall(
    retrieved_project_keys: &[String],
    retrieved_global_keys: &[String],
    expectations: RecallExpectations<'_>,
) -> f64 {
    let mut components = Vec::new();
    if !expectations.expected_project.is_empty() || !expectations.disallowed_project.is_empty() {
        components.push(component_score(
            retrieved_project_keys,
            expectations.expected_project,
            expectations.disallowed_project,
        ));
    }
    if !expectations.expected_global.is_empty() || !expectations.disallowed_global.is_empty() {
        components.push(component_score(
            retrieved_global_keys,
            expectations.expected_global,
            expectations.disallowed_global,
        ));
    }
    if components.is_empty() {
        100.0
    } else {
        round2(components.iter().sum::<f64>() / components.len() as f64)
    }
}

fn component_score(
    retrieved_keys: &[String],
    expected_keys: &[String],
    disallowed_keys: &[String],
) -> f64 {
    if expected_keys.is_empty() && disallowed_keys.is_empty() {
        return 100.0;
    }

    let rank = if expected_keys.is_empty() {
        100.0
    } else {
        rank_score(retrieved_keys, expected_keys)
    };
    let expected_set: std::collections::HashSet<&str> = expected_keys.iter().map(|s| s.as_str()).collect();
    let disallowed_set: std::collections::HashSet<&str> = disallowed_keys.iter().map(|s| s.as_str()).collect();
    let true_positives = retrieved_keys
        .iter()
        .filter(|k| expected_set.contains(k.as_str()))
        .count();
    let disallowed_hits = retrieved_keys
        .iter()
        .filter(|k| disallowed_set.contains(k.as_str()))
        .count();
    let precision = if retrieved_keys.is_empty() {
        if expected_keys.is_empty() { 100.0 } else { 0.0 }
    } else {
        100.0 * true_positives as f64 / retrieved_keys.len() as f64
    };
    let exclusion = if disallowed_keys.is_empty() {
        100.0
    } else {
        100.0 * (1.0 - (disallowed_hits as f64 / disallowed_keys.len() as f64)).max(0.0)
    };

    round2((rank * 0.6) + (precision * 0.2) + (exclusion * 0.2))
}

fn score_context(
    rendered_context: &str,
    required_context_substrings: &[String],
    forbidden_context_substrings: &[String],
) -> (f64, Vec<String>, Vec<String>) {
    let rendered_lower = rendered_context.to_lowercase();

    let missing_required_context: Vec<String> = required_context_substrings
        .iter()
        .filter(|needle| !rendered_lower.contains(&needle.to_lowercase()))
        .cloned()
        .collect();
    let present_forbidden_context: Vec<String> = forbidden_context_substrings
        .iter()
        .filter(|needle| rendered_lower.contains(&needle.to_lowercase()))
        .cloned()
        .collect();

    let required_score = if required_context_substrings.is_empty() {
        100.0
    } else {
        100.0
            * ((required_context_substrings.len() - missing_required_context.len()) as f64
                / required_context_substrings.len() as f64)
    };
    let forbidden_score = if forbidden_context_substrings.is_empty() {
        100.0
    } else {
        100.0
            * ((forbidden_context_substrings.len() - present_forbidden_context.len()) as f64
                / forbidden_context_substrings.len() as f64)
    };

    let mut parts = Vec::new();
    if !required_context_substrings.is_empty() {
        parts.push(required_score);
    }
    if !forbidden_context_substrings.is_empty() {
        parts.push(forbidden_score);
    }
    if parts.is_empty() {
        parts.push(100.0);
    }

    (
        round2(parts.iter().sum::<f64>() / parts.len() as f64),
        missing_required_context,
        present_forbidden_context,
    )
}

fn rank_score(retrieved_keys: &[String], expected_keys: &[String]) -> f64 {
    if expected_keys.is_empty() {
        return 100.0;
    }

    let total = expected_keys
        .iter()
        .map(|expected| {
            retrieved_keys
                .iter()
                .position(|actual| actual == expected)
                .map(|index| 1.0 / (index as f64 + 1.0))
                .unwrap_or(0.0)
        })
        .sum::<f64>();

    round2(100.0 * total / expected_keys.len() as f64)
}

fn score_hillclimb_case(
    retrieved_project_keys: &[String],
    retrieved_global_keys: &[String],
    rendered_context: &str,
    expectations: RecallExpectations<'_>,
    required_context_substrings: &[String],
    forbidden_context_substrings: &[String],
) -> f64 {
    let project_ndcg = ndcg_score(retrieved_project_keys, expectations.expected_project);
    let global_ndcg = ndcg_score(retrieved_global_keys, expectations.expected_global);
    let context_coverage = context_coverage_score(rendered_context, required_context_substrings);
    let context_cleanliness = context_cleanliness_score(rendered_context, forbidden_context_substrings);
    let retrieval_noise = count_unexpected_hits(retrieved_project_keys, expectations.expected_project)
        + count_unexpected_hits(retrieved_global_keys, expectations.expected_global);
    let disallowed_hits = count_disallowed_hits(retrieved_project_keys, expectations.disallowed_project)
        + count_disallowed_hits(retrieved_global_keys, expectations.disallowed_global);
    let extra_context_lines = extra_context_lines(rendered_context, required_context_substrings.len());

    round2(
        (project_ndcg * 8.0)
            + (global_ndcg * 5.0)
            + (context_coverage * 4.0)
            + (context_cleanliness * 3.0)
            - (retrieval_noise as f64 * 1.25)
            - (disallowed_hits as f64 * 2.0)
            - (extra_context_lines as f64 * 0.2),
    )
}

fn ndcg_score(retrieved_keys: &[String], expected_keys: &[String]) -> f64 {
    if expected_keys.is_empty() {
        return 1.0;
    }

    let dcg = expected_keys
        .iter()
        .filter_map(|expected| retrieved_keys.iter().position(|actual| actual == expected))
        .map(|index| 1.0 / ((index + 2) as f64).log2())
        .sum::<f64>();
    let idcg = (0..expected_keys.len())
        .map(|index| 1.0 / ((index + 2) as f64).log2())
        .sum::<f64>();

    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

fn context_coverage_score(rendered_context: &str, required_context_substrings: &[String]) -> f64 {
    if required_context_substrings.is_empty() {
        return 1.0;
    }
    let rendered_lower = rendered_context.to_lowercase();
    let covered = required_context_substrings
        .iter()
        .filter(|needle| rendered_lower.contains(&needle.to_lowercase()))
        .count();
    covered as f64 / required_context_substrings.len() as f64
}

fn context_cleanliness_score(rendered_context: &str, forbidden_context_substrings: &[String]) -> f64 {
    if forbidden_context_substrings.is_empty() {
        return 1.0;
    }
    let rendered_lower = rendered_context.to_lowercase();
    let forbidden_hits = forbidden_context_substrings
        .iter()
        .filter(|needle| rendered_lower.contains(&needle.to_lowercase()))
        .count();
    (1.0 - (forbidden_hits as f64 / forbidden_context_substrings.len() as f64)).max(0.0)
}

fn count_unexpected_hits(retrieved_keys: &[String], expected_keys: &[String]) -> usize {
    let expected: std::collections::HashSet<&str> = expected_keys.iter().map(|key| key.as_str()).collect();
    retrieved_keys
        .iter()
        .filter(|key| !expected.contains(key.as_str()))
        .count()
}

fn count_disallowed_hits(retrieved_keys: &[String], disallowed_keys: &[String]) -> usize {
    let disallowed: std::collections::HashSet<&str> =
        disallowed_keys.iter().map(|key| key.as_str()).collect();
    retrieved_keys
        .iter()
        .filter(|key| disallowed.contains(key.as_str()))
        .count()
}

fn extra_context_lines(rendered_context: &str, expected_required_count: usize) -> usize {
    let informative_lines = rendered_context
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- ["))
        .count();
    informative_lines.saturating_sub(expected_required_count.max(1))
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let collected: Vec<f64> = values.collect();
    if collected.is_empty() {
        0.0
    } else {
        round2(collected.iter().sum::<f64>() / collected.len() as f64)
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

struct ScratchSpace {
    project_dir: PathBuf,
    global_dir: PathBuf,
}

fn unique_nonce() -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_nanos();
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(format!("{now}-{counter}"))
}

impl ScratchSpace {
    fn new() -> Result<Self> {
        let nonce = unique_nonce()?;
        let root = std::env::temp_dir().join(format!("cortex-eval-run-{nonce}"));
        let project_dir = root.join("project");
        let global_dir = root.join("global");
        std::fs::create_dir_all(&project_dir)?;
        std::fs::create_dir_all(&global_dir)?;
        Ok(Self {
            project_dir,
            global_dir,
        })
    }
}

impl Drop for ScratchSpace {
    fn drop(&mut self) {
        if let Some(root) = self.project_dir.parent() {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_fixture(contents: &str) -> std::path::PathBuf {
        let nonce = unique_nonce().unwrap();
        let dir = std::env::temp_dir().join(format!("cortex-eval-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("benchmark.json");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn benchmark_scores_project_global_and_context_quality() {
        let path = write_fixture(
            r#"{
  "name": "memory-quality-smoke",
  "cases": [
    {
      "name": "project recall",
      "query": "upload race",
      "project_memories": [
        {
          "key": "upload_fix",
          "content": "Fixed race condition in upload handler by locking the temp file writer.",
          "type": "bugfix",
          "entities": [
            {"name": "upload handler", "type": "service"},
            {"name": "temp file writer", "type": "component"}
          ],
          "relationships": [
            {"source": "upload handler", "target": "temp file writer", "type": "uses"}
          ]
        },
        {
          "key": "db_choice",
          "content": "Chose SQLite over Postgres for simplicity in the local CLI prototype.",
          "type": "decision",
          "entities": [
            {"name": "SQLite", "type": "technology"},
            {"name": "Postgres", "type": "technology"}
          ],
          "relationships": [
            {"source": "SQLite", "target": "Postgres", "type": "alternative_to"}
          ]
        }
      ],
      "expected_project_keys": ["upload_fix"],
      "required_context_substrings": ["race condition in upload handler"],
      "forbidden_context_substrings": ["Global Knowledge"]
    },
    {
      "name": "global preference recall",
      "query": "rust preference",
      "global_memories": [
        {
          "key": "lang_pref",
          "content": "I prefer Rust and Go for CLI tools.",
          "type": "preference",
          "entities": [
            {"name": "Rust", "type": "language"},
            {"name": "Go", "type": "language"}
          ]
        }
      ],
      "expected_global_keys": ["lang_pref"],
      "required_context_substrings": ["I prefer Rust and Go for CLI tools."]
    }
  ]
}"#,
        );

        let report = run_benchmark_file(&path).unwrap();
        assert_eq!(report.benchmark, "memory-quality-smoke");
        assert_eq!(report.case_reports.len(), 2);
        assert!(report.total_score > 80.0, "unexpected report: {report:#?}");
        assert!(report.recall_score > 75.0, "unexpected report: {report:#?}");
        assert!(report.context_score > 75.0, "unexpected report: {report:#?}");
    }

    #[test]
    fn recall_score_penalizes_irrelevant_extra_hits() {
        let precise = score_recall(
            &["upload_fix".to_string()],
            &[],
            RecallExpectations {
                expected_project: &["upload_fix".to_string()],
                ..Default::default()
            },
        );
        let noisy = score_recall(
            &["upload_fix".to_string(), "distractor".to_string()],
            &[],
            RecallExpectations {
                expected_project: &["upload_fix".to_string()],
                ..Default::default()
            },
        );
        assert!(precise > noisy, "precise={precise}, noisy={noisy}");
    }

    #[test]
    fn disallowed_keys_are_penalized_for_contradictions() {
        let resolved = score_recall(
            &["current_policy".to_string()],
            &[],
            RecallExpectations {
                expected_project: &["current_policy".to_string()],
                disallowed_project: &["old_policy".to_string()],
                ..Default::default()
            },
        );
        let contradictory = score_recall(
            &["current_policy".to_string(), "old_policy".to_string()],
            &[],
            RecallExpectations {
                expected_project: &["current_policy".to_string()],
                disallowed_project: &["old_policy".to_string()],
                ..Default::default()
            },
        );
        assert!(resolved > contradictory, "resolved={resolved}, contradictory={contradictory}");
    }

    #[test]
    fn superseded_memories_are_hidden_from_search_and_context() {
        let scratch = ScratchSpace::new().unwrap();
        let raw_conn = db::open_raw_db(&scratch.project_dir.join("raw.db")).unwrap();
        let cons_conn = db::open_consolidated_db(&scratch.project_dir.join("consolidated.db")).unwrap();

        let old_id = db::insert_consolidated(
            &cons_conn,
            "Upload retry policy uses exponential backoff up to 30 seconds.",
            "decision",
            &[],
            0.8,
        )
        .unwrap();
        let new_id = db::insert_consolidated(
            &cons_conn,
            "Upload retry policy now caps backoff at 5 seconds to preserve snappy UX.",
            "decision",
            &[],
            1.0,
        )
        .unwrap();
        db::mark_consolidated_superseded(&cons_conn, old_id, new_id).unwrap();

        let search = db::search_consolidated(&cons_conn, "retry policy upload", 5).unwrap();
        assert_eq!(search.iter().map(|m| m.id).collect::<Vec<_>>(), vec![new_id]);

        let ctx = context::format_context(&cons_conn, &raw_conn, None, false, Some("retry policy upload"), 5).unwrap();
        assert!(ctx.contains("caps backoff at 5 seconds"), "ctx={ctx}");
        assert!(!ctx.contains("up to 30 seconds"), "ctx={ctx}");
    }

    #[test]
    fn contradiction_case_prefers_current_policy_only() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|c| c.name == "contradiction recall should prefer current policy")
            .unwrap();
        assert_eq!(case.retrieved_project_keys, vec!["current_policy".to_string()]);
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn project_specific_preference_beats_generic_global_preference() {
        let scratch = ScratchSpace::new().unwrap();
        let raw_conn = db::open_raw_db(&scratch.project_dir.join("raw.db")).unwrap();
        let cons_conn = db::open_consolidated_db(&scratch.project_dir.join("consolidated.db")).unwrap();
        let global_cons = db::open_consolidated_db(&scratch.global_dir.join("consolidated.db")).unwrap();

        let raw_id = db::save_memory(
            &raw_conn,
            "For uploads, prefer resumable chunked transfers over single-shot uploads.",
            "preference",
            "eval",
        )
        .unwrap();
        db::insert_consolidated(
            &cons_conn,
            "For uploads, prefer resumable chunked transfers over single-shot uploads.",
            "preference",
            &[raw_id],
            1.0,
        )
        .unwrap();
        db::insert_consolidated(
            &global_cons,
            "I prefer Rust and Go for CLI tools.",
            "preference",
            &[],
            1.0,
        )
        .unwrap();

        let ctx = context::format_context(
            &cons_conn,
            &raw_conn,
            Some(&global_cons),
            false,
            Some("upload preference"),
            2,
        )
        .unwrap();

        assert!(ctx.contains("prefer resumable chunked transfers"), "ctx={ctx}");
        assert!(!ctx.contains("Rust and Go for CLI tools"), "ctx={ctx}");
    }

    #[test]
    fn confidence_controls_top_rank_for_trace_like_preferences() {
        let path = write_fixture(
            r#"{
  "name": "confidence-ranking",
  "cases": [
    {
      "name": "preferential global ranking",
      "query": "cli preference",
      "limit": 1,
      "global_memories": [
        {
          "key": "old_python_pref",
          "content": "I prefer Python for quick CLI tools.",
          "type": "preference",
          "confidence": 0.3
        },
        {
          "key": "current_rust_pref",
          "content": "I prefer Rust and Go for CLI tools.",
          "type": "preference",
          "confidence": 1.0
        }
      ],
      "expected_global_keys": ["current_rust_pref"],
      "required_context_substrings": ["Rust and Go for CLI tools"],
      "forbidden_context_substrings": ["Python for quick CLI tools"]
    }
  ]
}"#,
        );

        let report = run_benchmark_file(&path).unwrap();
        assert_eq!(report.case_reports[0].retrieved_global_keys, vec!["current_rust_pref".to_string()]);
        assert!(report.total_score >= 90.0, "unexpected report: {report:#?}");
    }

    #[test]
    fn weak_global_partial_match_should_not_leak_into_strong_project_context() {
        let scratch = ScratchSpace::new().unwrap();
        let raw_conn = db::open_raw_db(&scratch.project_dir.join("raw.db")).unwrap();
        let cons_conn = db::open_consolidated_db(&scratch.project_dir.join("consolidated.db")).unwrap();
        let global_cons = db::open_consolidated_db(&scratch.global_dir.join("consolidated.db")).unwrap();

        let raw_id = db::save_memory(
            &raw_conn,
            "For uploads, prefer a 5 second retry cap with resumable chunked retries.",
            "preference",
            "eval",
        )
        .unwrap();
        db::insert_consolidated(
            &cons_conn,
            "For uploads, prefer a 5 second retry cap with resumable chunked retries.",
            "preference",
            &[raw_id],
            1.0,
        )
        .unwrap();
        db::insert_consolidated(
            &global_cons,
            "I prefer concise terminal tooling for daily work.",
            "preference",
            &[],
            1.0,
        )
        .unwrap();

        let ctx = context::format_context(
            &cons_conn,
            &raw_conn,
            Some(&global_cons),
            false,
            Some("upload retry preference"),
            3,
        )
        .unwrap();

        assert!(ctx.contains("prefer a 5 second retry cap"), "ctx={ctx}");
        assert!(!ctx.contains("Global Knowledge"), "ctx={ctx}");
        assert!(!ctx.contains("concise terminal tooling"), "ctx={ctx}");
    }

    #[test]
    fn domain_specific_preference_query_should_not_surface_generic_global_preference() {
        let scratch = ScratchSpace::new().unwrap();
        let raw_conn = db::open_raw_db(&scratch.project_dir.join("raw.db")).unwrap();
        let cons_conn = db::open_consolidated_db(&scratch.project_dir.join("consolidated.db")).unwrap();
        let global_cons = db::open_consolidated_db(&scratch.global_dir.join("consolidated.db")).unwrap();

        let raw_id = db::save_memory(
            &raw_conn,
            "For uploads, prefer a 5 second retry cap with resumable chunked retries.",
            "preference",
            "eval",
        )
        .unwrap();
        db::insert_consolidated(
            &cons_conn,
            "For uploads, prefer a 5 second retry cap with resumable chunked retries.",
            "preference",
            &[raw_id],
            1.0,
        )
        .unwrap();
        db::insert_consolidated(
            &global_cons,
            "I prefer Rust and Go for CLI tools.",
            "preference",
            &[],
            1.0,
        )
        .unwrap();

        let global_hits = db::search_consolidated(&global_cons, "prefer upload retries", 3).unwrap();
        assert!(global_hits.is_empty(), "global_hits={global_hits:#?}");

        let ctx = context::format_context(
            &cons_conn,
            &raw_conn,
            Some(&global_cons),
            false,
            Some("prefer upload retries"),
            3,
        )
        .unwrap();

        assert!(ctx.contains("prefer a 5 second retry cap"), "ctx={ctx}");
        assert!(!ctx.contains("Global Knowledge"), "ctx={ctx}");
        assert!(!ctx.contains("Rust and Go for CLI tools"), "ctx={ctx}");
    }

    #[test]
    fn near_match_global_distractor_should_stay_out_of_project_context() {
        let scratch = ScratchSpace::new().unwrap();
        let raw_conn = db::open_raw_db(&scratch.project_dir.join("raw.db")).unwrap();
        let cons_conn = db::open_consolidated_db(&scratch.project_dir.join("consolidated.db")).unwrap();
        let global_cons = db::open_consolidated_db(&scratch.global_dir.join("consolidated.db")).unwrap();

        let raw_id = db::save_memory(
            &raw_conn,
            "Upload retry cap should stay at 5 seconds to preserve snappy UX.",
            "decision",
            "eval",
        )
        .unwrap();
        db::insert_consolidated(
            &cons_conn,
            "Upload retry cap should stay at 5 seconds to preserve snappy UX.",
            "decision",
            &[raw_id],
            1.0,
        )
        .unwrap();
        db::insert_consolidated(
            &global_cons,
            "API retry cap can extend to 30 seconds for long-running sync jobs.",
            "decision",
            &[],
            1.0,
        )
        .unwrap();

        let ctx = context::format_context(
            &cons_conn,
            &raw_conn,
            Some(&global_cons),
            false,
            Some("upload retry cap"),
            3,
        )
        .unwrap();

        assert!(ctx.contains("Upload retry cap should stay at 5 seconds"), "ctx={ctx}");
        assert!(!ctx.contains("Global Knowledge"), "ctx={ctx}");
        assert!(!ctx.contains("30 seconds for long-running sync jobs"), "ctx={ctx}");
    }

    #[test]
    fn hillclimb_score_penalizes_noise_and_rewards_tighter_rankings() {
        let precise = score_hillclimb_case(
            &["upload_fix".to_string()],
            &[],
            "### Learned Patterns\n- [bugfix] Fixed race condition in upload handler by locking the temp file writer.\n",
            RecallExpectations {
                expected_project: &["upload_fix".to_string()],
                ..Default::default()
            },
            &["race condition in upload handler".to_string()],
            &[],
        );
        let noisy = score_hillclimb_case(
            &["upload_fix".to_string(), "upload_perf".to_string(), "ui_race".to_string()],
            &[],
            "### Learned Patterns\n- [bugfix] Fixed race condition in upload handler by locking the temp file writer.\n- [pattern] Upload throughput improved after increasing worker count and removing lock contention in the preview generator.\n- [bugfix] A UI race appears when the upload progress component rerenders during reconnect.\n",
            RecallExpectations {
                expected_project: &["upload_fix".to_string()],
                ..Default::default()
            },
            &["race condition in upload handler".to_string()],
            &["preview generator".to_string(), "upload progress component".to_string()],
        );

        assert!(precise > noisy, "precise={precise}, noisy={noisy}");
    }

    #[test]
    fn entity_recall_case_suppresses_neighbor_only_distractor() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "entity recall should suppress neighbor-only distractor")
            .unwrap();
        assert_eq!(case.retrieved_project_keys, vec!["userlist_eager".to_string()]);
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn morphology_aware_retrieval_prefers_canonical_retry_memory() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "morphology-aware retrieval should prefer canonical retry memory")
            .unwrap();
        assert_eq!(
            case.retrieved_project_keys,
            vec!["retry_503".to_string()],
            "unexpected case: {case:#?}"
        );
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn entity_recall_should_rank_concise_canonical_hit_above_equally_matched_distractor() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "entity recall should rank concise canonical hit above equally matched distractor")
            .unwrap();
        assert_eq!(
            case.retrieved_project_keys,
            vec!["userlist_eager_queries".to_string()],
            "unexpected case: {case:#?}"
        );
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn hyphenated_query_matches_spaced_canonical_phrase() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "hyphenated query should match spaced canonical phrase")
            .unwrap();
        assert_eq!(case.retrieved_project_keys, vec!["temp_file_writer_lock".to_string()]);
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn snake_case_query_matches_spaced_canonical_phrase() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "snake_case query should match spaced canonical phrase")
            .unwrap();
        assert_eq!(case.retrieved_project_keys, vec!["temp_file_writer_lock".to_string()]);
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn morphology_case_prefers_canonical_retry_memory() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "morphology-aware retrieval should prefer canonical retry memory")
            .unwrap();
        assert_eq!(case.retrieved_project_keys, vec!["retry_503".to_string()]);
        assert!(case.present_forbidden_context.is_empty(), "unexpected case: {case:#?}");
    }

    #[test]
    fn benchmark_retrieval_aligns_with_context_routing() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();

        let global_pref_case = report
            .case_reports
            .iter()
            .find(|case| case.name == "global preference recall with project distractor")
            .unwrap();
        assert!(
            global_pref_case.retrieved_project_keys.is_empty(),
            "unexpected case: {global_pref_case:#?}"
        );

        let near_match_case = report
            .case_reports
            .iter()
            .find(|case| case.name == "near-match global distractor should stay out of project context")
            .unwrap();
        assert!(
            near_match_case.retrieved_global_keys.is_empty(),
            "unexpected case: {near_match_case:#?}"
        );
    }

    #[test]
    fn exact_project_phrase_should_beat_same_term_global_distractor() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        let case = report
            .case_reports
            .iter()
            .find(|case| case.name == "exact project phrase should beat same-term global distractor")
            .unwrap();
        assert_eq!(
            case.retrieved_project_keys,
            vec!["upload_cap_exact".to_string()]
        );
        assert!(
            case.retrieved_global_keys.is_empty(),
            "unexpected case: {case:#?}"
        );
        assert!(
            case.present_forbidden_context.is_empty(),
            "unexpected case: {case:#?}"
        );
    }

    #[test]
    fn benchmark_report_includes_hillclimb_score() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        assert!(report.hillclimb_score > 0.0, "unexpected report: {report:#?}");
        assert!(
            report.case_reports.iter().all(|case| case.hillclimb_score > 0.0),
            "unexpected report: {report:#?}"
        );
    }

    #[test]
    fn benchmark_v2_stays_above_quality_bar() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("eval")
            .join("benchmark.json");
        let report = run_benchmark_file(&path).unwrap();
        assert!(report.total_score >= 90.0, "unexpected report: {report:#?}");
        assert!(report.context_score >= 85.0, "unexpected report: {report:#?}");
    }
}
