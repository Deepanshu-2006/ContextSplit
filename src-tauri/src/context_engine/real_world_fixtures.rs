//! ContextSplit Real-World Context Test Fixtures
//!
//! Provides structured, realistic context profiles and multi-signal test scenarios
//! for Day 29 Real-World Mac Testing:
//! 1. ContextSplit Development
//! 2. PayX Development
//! 3. University (CS4500 Operating Systems)
//! 4. Personal / General
//!
//! ARCHITECTURAL INVARIANT:
//! Applications alone (VS Code, Chrome, Xcode, Terminal) remain weak/general evidence
//! and do NOT dictate context association without corroborating paths, domains, or window metadata.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::models::Workspace;
use crate::db::operations::create_workspace;

pub const REALISTIC_CONTEXTS_JSON: &str = include_str!("../../../tests/fixtures/realistic_contexts.json");

/// Identifying signals configured for a realistic workspace context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealisticContextSignals {
    pub file_paths: Vec<String>,
    pub window_title_patterns: Vec<String>,
    pub browser_domains: Vec<String>,
    pub browser_urls: Vec<String>,
    pub task_patterns: Vec<String>,
}

/// Structured definition of a realistic workspace context fixture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealisticContextFixture {
    pub id: String,
    pub name: String,
    pub description: String,
    pub repository_root: Option<String>,
    pub git_branches: Vec<String>,
    pub primary_languages: Vec<String>,
    pub identifying_signals: RealisticContextSignals,
}

/// Scenario inputs for combination testing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioInputs {
    pub app_name: String,
    pub window_title: String,
    pub file_path: Option<String>,
    pub domain: Option<String>,
    pub url: Option<String>,
}

/// Expected results for a combination test scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioExpected {
    pub primary_context_id: Option<String>,
    pub rejected_context_id: Option<String>,
    pub min_confidence: Option<f32>,
    pub max_confidence_for_professional_contexts: Option<f32>,
    pub is_unambiguous: Option<bool>,
    pub is_general_noise: Option<bool>,
}

/// Combination test scenario fixture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombinationScenarioFixture {
    pub id: String,
    pub description: String,
    pub inputs: ScenarioInputs,
    pub expected: ScenarioExpected,
}

/// Root container for realistic context fixtures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealisticFixturesDataset {
    pub version: String,
    pub description: String,
    pub contexts: Vec<RealisticContextFixture>,
    pub combination_scenarios: Vec<CombinationScenarioFixture>,
}

impl RealisticFixturesDataset {
    /// Loads and parses the bundled realistic contexts fixture dataset.
    pub fn load() -> Self {
        serde_json::from_str(REALISTIC_CONTEXTS_JSON)
            .expect("realistic_contexts.json must be valid JSON")
    }

    /// Finds a context fixture by its ID.
    pub fn get_context(&self, id: &str) -> Option<&RealisticContextFixture> {
        self.contexts.iter().find(|c| c.id == id)
    }

    /// Seeds an in-memory SQLite database with the realistic workspace contexts.
    pub fn seed_database(&self, conn: &Connection) -> Vec<Workspace> {
        let mut workspaces = Vec::new();
        for ctx in &self.contexts {
            let ws = create_workspace(conn, &ctx.name).expect("Failed to create workspace");
            workspaces.push(ws);
        }
        workspaces
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_realistic_fixtures_dataset_contains_all_four_contexts() {
        let dataset = RealisticFixturesDataset::load();
        assert_eq!(dataset.contexts.len(), 4, "Must contain exactly 4 realistic contexts");

        let ids: Vec<&str> = dataset.contexts.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"ws_contextsplit"));
        assert!(ids.contains(&"ws_payx"));
        assert!(ids.contains(&"ws_university"));
        assert!(ids.contains(&"ws_personal_general"));

        assert!(dataset.combination_scenarios.len() >= 6, "Must contain realistic combination scenarios");
    }

    #[test]
    fn test_seed_database_with_realistic_contexts() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::run_migrations(&conn).unwrap();

        let dataset = RealisticFixturesDataset::load();
        let workspaces = dataset.seed_database(&conn);
        assert_eq!(workspaces.len(), 4);

        let mut stmt = conn.prepare("SELECT name FROM workspaces ORDER BY name").unwrap();
        let names: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();

        assert!(names.contains(&"ContextSplit Development".to_string()));
        assert!(names.contains(&"PayX Development".to_string()));
        assert!(names.contains(&"University".to_string()));
        assert!(names.contains(&"Personal / General".to_string()));
    }

    #[test]
    fn test_context_combinations_and_app_alone_weak_evidence_invariant() {
        let dataset = RealisticFixturesDataset::load();

        // 1. VS Code with University assignment must not match ContextSplit
        let uni_scenario = dataset.combination_scenarios.iter().find(|s| s.id == "scenario_misleading_common_app_vscode_in_university").unwrap();
        assert_eq!(uni_scenario.inputs.app_name, "Visual Studio Code");
        assert_eq!(uni_scenario.expected.primary_context_id.as_deref(), Some("ws_university"));
        assert_eq!(uni_scenario.expected.rejected_context_id.as_deref(), Some("ws_contextsplit"));

        // 2. Chrome with Tauri docs must match ContextSplit, not University
        let chrome_scenario = dataset.combination_scenarios.iter().find(|s| s.id == "scenario_misleading_common_app_chrome_in_contextsplit").unwrap();
        assert_eq!(chrome_scenario.inputs.app_name, "Google Chrome");
        assert_eq!(chrome_scenario.expected.primary_context_id.as_deref(), Some("ws_contextsplit"));
        assert_eq!(chrome_scenario.expected.rejected_context_id.as_deref(), Some("ws_university"));

        // 3. YouTube lofi beats in Chrome must be classified as general noise, not any professional workspace
        let personal_scenario = dataset.combination_scenarios.iter().find(|s| s.id == "scenario_personal_unrelated_browsing").unwrap();
        assert_eq!(personal_scenario.expected.primary_context_id, None);
        assert_eq!(personal_scenario.expected.is_general_noise, Some(true));
    }
}
