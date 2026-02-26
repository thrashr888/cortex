use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub content: String,
    pub r#type: String,
    pub created_at: String,
    pub accessed_at: String,
    pub access_count: i64,
    pub consolidated: bool,
    pub importance: f64,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedMemory {
    pub id: i64,
    pub content: String,
    pub r#type: String,
    pub source_ids: Vec<i64>,
    pub confidence: f64,
    pub created_at: String,
    pub updated_at: String,
    pub access_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: i64,
    pub name: String,
    pub content: String,
    pub source_ids: Vec<i64>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub raw_count: i64,
    pub unconsolidated_count: i64,
    pub consolidated_count: i64,
    pub skill_count: i64,
    pub last_sleep: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consolidation {
    pub content: String,
    pub r#type: String,
    pub source_ids: Vec<i64>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub old_id: i64,
    pub new_id: i64,
    pub resolution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdate {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalPromotion {
    pub content: String,
    pub r#type: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationResult {
    #[serde(default)]
    pub consolidations: Vec<Consolidation>,
    #[serde(default)]
    pub contradictions: Vec<Contradiction>,
    #[serde(default)]
    pub promotions: Vec<i64>,
    #[serde(default)]
    pub decayed: Vec<i64>,
    #[serde(default)]
    pub skill_updates: Vec<SkillUpdate>,
    #[serde(default)]
    pub global_promotions: Vec<GlobalPromotion>,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Memories: {} total ({} unconsolidated)", self.raw_count, self.unconsolidated_count)?;
        writeln!(f, "Consolidated: {}", self.consolidated_count)?;
        writeln!(f, "Skills: {}", self.skill_count)?;
        if let Some(ref last) = self.last_sleep {
            write!(f, "Last sleep: {}", last)?;
        } else {
            write!(f, "Last sleep: never")?;
        }
        Ok(())
    }
}
