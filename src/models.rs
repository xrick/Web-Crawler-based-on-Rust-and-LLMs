//! Serializable records used by the worker, SQLite, and the HTTP API.
use serde::{Deserialize, Serialize};

pub const CATEGORIES: [&str; 5] = ["iphone", "mac", "ipad", "watch", "airpods"];
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub categories: Vec<String>,
    pub model: String,
    pub max_pages: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            categories: CATEGORIES.iter().map(|s| s.to_string()).collect(),
            model: "qwen2.5-coder:14b".into(),
            max_pages: 100,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.categories.is_empty()
            || self
                .categories
                .iter()
                .any(|s| !CATEGORIES.contains(&s.as_str()))
        {
            return Err("請選擇有效產品類別".into());
        }
        if !(5..=300).contains(&self.max_pages) {
            return Err("頁數上限需介於 5 至 300".into());
        }
        if self.model.is_empty() || self.model.len() > 150 {
            return Err("請選擇模型".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOptions {
    #[serde(default)]
    pub discovery_only: bool,
    // Useful for a reproducible five-category smoke run; 0 means all discovered products.
    #[serde(default)]
    pub products_per_category: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub url: String,
    pub stage: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRecord {
    pub url: String,
    pub kind: String,
    pub category: String,
    pub fetched_at: String,
    pub html_file: String,
    pub text_file: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub category: String,
    pub url: String,
    pub specs_url: Option<String>,
    pub buy_url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: usize,
    pub section: String,
    pub model_hint: Option<String>,
    pub context: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub amount: f64,
    pub currency: String,
    pub source_url: String,
    pub source_name: String,
    pub evidence: String,
    pub scope: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Specification {
    pub block_id: usize,
    pub section: String,
    pub label: String,
    pub value: String,
    pub model: Option<String>,
    pub conditions: Option<String>,
    pub evidence: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub category: String,
    pub name: String,
    pub product_url: String,
    pub specs_url: String,
    pub fetched_at: String,
    pub model: String,
    pub variants: Vec<String>,
    pub starting_price: Option<Price>,
    #[serde(default)]
    pub model_prices: Vec<Price>,
    pub specs: Vec<Specification>,
    pub blocks: Vec<Block>,
    pub llm_calls: usize,
    pub llm_ms: u128,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub status: String,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub settings: Settings,
    pub options: RunOptions,
    pub discovered: usize,
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub current_url: String,
    pub pages: Vec<PageRecord>,
    pub candidates: Vec<Candidate>,
    pub products: Vec<Product>,
    pub issues: Vec<Issue>,
}
impl Job {
    pub fn new(settings: Settings, options: RunOptions) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            status: "running".into(),
            phase: "準備".into(),
            started_at: now(),
            finished_at: None,
            settings,
            options,
            discovered: 0,
            processed: 0,
            succeeded: 0,
            failed: 0,
            current_url: String::new(),
            pages: vec![],
            candidates: vec![],
            products: vec![],
            issues: vec![],
        }
    }
    pub fn issue(&mut self, url: &str, stage: &str, message: impl ToString) {
        self.issues.push(Issue {
            url: url.into(),
            stage: stage.into(),
            message: message.to_string(),
        });
    }
}
