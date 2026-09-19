use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result as SqlResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::db::models::{Item, Workspace};

// =========================================================================
// Contamination Categories & Enums
// =========================================================================

/// Explicit classification of context contamination risks.
///
/// Invariant: Context assignment requires contextual evidence, not application identity alone.
/// Unrelated activities (entertainment, social media, unrelated searches, unrelated repos/files)
/// must NEVER contaminate an active context purely due to temporal/session proximity or app overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContaminationCategory {
    /// Video, music, or streaming entertainment (YouTube, Netflix, Spotify, Twitch, TikTok, etc.)
    EntertainmentMedia,
    /// Social networking platforms (Twitter/X, Instagram, Reddit, Facebook, Threads, etc.)
    SocialMedia,
    /// Generic search engine query lacking semantic overlap with the active workspace (Google, Bing, DuckDuckGo)
    UnrelatedSearchQuery,
    /// Code repository completely unrelated to workspace domain or tasks (e.g. GitHub repo during Tokyo Trip)
    UnrelatedRepository,
    /// Local file or personal document with mismatched purpose/extension (e.g. Tax return during OS assignment)
    UnrelatedFile,
    /// Generic or multi-purpose website (Amazon, eBay, Wikipedia, news sites) without topical relevance
    GenericDomainOrShopping,
    /// Multi-purpose host application (Chrome, Safari, VS Code, Terminal) without content corroboration
    UncorroboratedCommonApplication,
}

impl ContaminationCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EntertainmentMedia => "entertainment_media",
            Self::SocialMedia => "social_media",
            Self::UnrelatedSearchQuery => "unrelated_search_query",
            Self::UnrelatedRepository => "unrelated_repository",
            Self::UnrelatedFile => "unrelated_file",
            Self::GenericDomainOrShopping => "generic_domain_or_shopping",
            Self::UncorroboratedCommonApplication => "uncorroborated_common_application",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::EntertainmentMedia => "Entertainment / Streaming Media",
            Self::SocialMedia => "Social Media Activity",
            Self::UnrelatedSearchQuery => "Unrelated Search Query",
            Self::UnrelatedRepository => "Unrelated Code Repository",
            Self::UnrelatedFile => "Unrelated Local File",
            Self::GenericDomainOrShopping => "Generic Domain / Casual Web Browsing",
            Self::UncorroboratedCommonApplication => "Uncorroborated Application Overlap",
        }
    }

    /// Base contamination penalty weight [0.0, 1.0].
    pub fn default_penalty(&self) -> f64 {
        match self {
            Self::EntertainmentMedia => 0.75,
            Self::SocialMedia => 0.70,
            Self::UnrelatedSearchQuery => 0.65,
            Self::UnrelatedRepository => 0.70,
            Self::UnrelatedFile => 0.65,
            Self::GenericDomainOrShopping => 0.55,
            Self::UncorroboratedCommonApplication => 0.45,
        }
    }
}

impl std::fmt::Display for ContaminationCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =========================================================================
// Contamination Evaluation Input & Output
// =========================================================================

/// Input parameters for evaluating whether an activity risks contaminating a target workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContaminationCheckInput {
    pub activity_id: Option<String>,
    pub title: String,
    pub content: Option<String>,
    pub app_name: Option<String>,
    pub domain: Option<String>,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub target_workspace_id: String,
    pub target_workspace_name: String,
    pub target_workspace_keywords: Vec<String>,
    pub target_workspace_domains: Vec<String>,
    pub target_workspace_repos: Vec<String>,
    pub target_workspace_active_tasks: Vec<String>,
    pub target_workspace_file_extensions: Vec<String>,
    pub semantic_similarity_score: f64,
    pub is_explicit_user_action: bool,
}

impl ContaminationCheckInput {
    pub fn new(
        title: impl Into<String>,
        target_ws_id: impl Into<String>,
        target_ws_name: impl Into<String>,
    ) -> Self {
        let ws_name = target_ws_name.into();
        let keywords = extract_topic_tokens(&ws_name);
        Self {
            activity_id: None,
            title: title.into(),
            content: None,
            app_name: None,
            domain: None,
            url: None,
            file_path: None,
            target_workspace_id: target_ws_id.into(),
            target_workspace_name: ws_name,
            target_workspace_keywords: keywords,
            target_workspace_domains: Vec::new(),
            target_workspace_repos: Vec::new(),
            target_workspace_active_tasks: Vec::new(),
            target_workspace_file_extensions: Vec::new(),
            semantic_similarity_score: 0.0,
            is_explicit_user_action: false,
        }
    }

    pub fn with_app(mut self, app: impl Into<String>) -> Self {
        self.app_name = Some(app.into());
        self
    }

    pub fn with_domain(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    pub fn with_workspace_details(
        mut self,
        keywords: Vec<String>,
        domains: Vec<String>,
        repos: Vec<String>,
        tasks: Vec<String>,
    ) -> Self {
        for k in keywords {
            if !self.target_workspace_keywords.contains(&k) {
                self.target_workspace_keywords.push(k);
            }
        }
        self.target_workspace_domains = domains;
        self.target_workspace_repos = repos;
        self.target_workspace_active_tasks = tasks;
        self
    }

    pub fn with_semantic_score(mut self, score: f64) -> Self {
        self.semantic_similarity_score = score.clamp(0.0, 1.0);
        self
    }

    pub fn with_explicit_user_action(mut self, is_explicit: bool) -> Self {
        self.is_explicit_user_action = is_explicit;
        self
    }
}

/// Detailed result of anti-contamination analysis for an activity against a workspace context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContaminationDefenseResult {
    /// Whether this activity is deemed an unrelated contamination risk
    pub is_contamination_risk: bool,
    /// Primary contamination category identified
    pub category: Option<ContaminationCategory>,
    /// Calculated anti-contamination penalty [0.0, 1.0]
    pub contamination_penalty: f64,
    /// Whether this assignment is relying purely on application identity without content corroboration
    pub is_uncorroborated_app_only: bool,
    /// Recommended strict rejection from the target workspace
    pub should_reject_from_context: bool,
    /// Safe routing: should be held in Inbox instead of modifying working context
    pub should_hold_in_inbox: bool,
    /// Primary explanation for user transparency and audit logs
    pub explanation: String,
    /// Detailed diagnostic reasons
    pub reasons: Vec<String>,
}

impl Default for ContaminationDefenseResult {
    fn default() -> Self {
        Self {
            is_contamination_risk: false,
            category: None,
            contamination_penalty: 0.0,
            is_uncorroborated_app_only: false,
            should_reject_from_context: false,
            should_hold_in_inbox: false,
            explanation: "No context contamination risk detected.".to_string(),
            reasons: Vec::new(),
        }
    }
}

// =========================================================================
// Known Distraction & Domain Signatures
// =========================================================================

const ENTERTAINMENT_DOMAINS: &[&str] = &[
    "youtube.com",
    "youtu.be",
    "m.youtube.com",
    "netflix.com",
    "spotify.com",
    "open.spotify.com",
    "twitch.tv",
    "vimeo.com",
    "tiktok.com",
    "hulu.com",
    "disneyplus.com",
    "primevideo.com",
    "hbomax.com",
    "max.com",
    "crunchyroll.com",
    "soundcloud.com",
    "dailymotion.com",
    "9gag.com",
    "steamcommunity.com",
    "store.steampowered.com",
    "epicgames.com",
    "roblox.com",
    "chess.com",
    "ign.com",
    "gamespot.com",
];

const ENTERTAINMENT_APPS: &[&str] = &[
    "Spotify",
    "Steam",
    "Discord",
    "Music",
    "VLC",
    "QuickTime Player",
    "Podcasts",
    "TV",
    "Battle.net",
    "Epic Games Launcher",
    "Roblox",
];

const SOCIAL_MEDIA_DOMAINS: &[&str] = &[
    "twitter.com",
    "x.com",
    "instagram.com",
    "facebook.com",
    "reddit.com",
    "old.reddit.com",
    "threads.net",
    "pinterest.com",
    "tumblr.com",
    "snapchat.com",
    "bsky.app",
    "mastodon.social",
    "quora.com",
];

const SEARCH_ENGINE_DOMAINS: &[&str] = &[
    "google.com",
    "google.co",
    "google.co.uk",
    "google.co.in",
    "google.ca",
    "google.com.au",
    "google.de",
    "google.fr",
    "google.es",
    "google.it",
    "bing.com",
    "duckduckgo.com",
    "search.yahoo.com",
    "ecosia.org",
    "kagi.com",
    "baidu.com",
    "startpage.com",
    "search.brave.com",
];

const GENERIC_SHOPPING_AND_NEWS_DOMAINS: &[&str] = &[
    "amazon.com",
    "amazon.co.uk",
    "amazon.in",
    "ebay.com",
    "walmart.com",
    "target.com",
    "aliexpress.com",
    "bestbuy.com",
    "etsy.com",
    "shein.com",
    "zara.com",
    "wikipedia.org",
    "en.wikipedia.org",
    "medium.com",
    "news.ycombinator.com",
    "cnn.com",
    "bbc.com",
    "bbc.co.uk",
    "nytimes.com",
    "theverge.com",
    "wired.com",
    "techcrunch.com",
    "reddit.com",
];

const COMMON_HOST_APPS: &[&str] = &[
    "Google Chrome",
    "Chrome",
    "Safari",
    "Arc",
    "Brave Browser",
    "Brave",
    "Microsoft Edge",
    "Edge",
    "Firefox",
    "Opera",
    "Vivaldi",
    "Chromium",
    "Visual Studio Code",
    "Code",
    "VS Code",
    "VSCode",
    "Cursor",
    "Windsurf",
    "Zed",
    "Sublime Text",
    "TextEdit",
    "Atom",
    "Xcode",
    "Android Studio",
    "IntelliJ IDEA",
    "IntelliJ",
    "PyCharm",
    "WebStorm",
    "CLion",
    "GoLand",
    "RustRover",
    "Rider",
    "DataGrip",
    "Figma",
    "Canva",
    "Sketch",
    "Adobe XD",
    "Photoshop",
    "Illustrator",
    "InDesign",
    "Terminal",
    "iTerm2",
    "iTerm",
    "Alacritty",
    "Kitty",
    "Warp",
    "Hyper",
    "Finder",
    "Explorer",
    "Notes",
    "Preview",
    "GitHub Desktop",
    "GitKraken",
    "SourceTree",
    "Tower",
    "Postman",
    "Insomnia",
    "TablePlus",
    "DBeaver",
    "Slack",
    "Discord",
    "Notion",
    "Obsidian",
    "Linear",
    "Jira",
    "Trello",
    "Asana",
];

// =========================================================================
// Anti-Contamination Guard Evaluator
// =========================================================================

/// Evaluator that defends workspaces against unrelated activity contamination.
#[derive(Debug, Clone, Default)]
pub struct ContaminationGuard;

impl ContaminationGuard {
    pub fn new() -> Self {
        Self
    }

    /// Evaluates whether an activity poses a context contamination risk to a target workspace.
    pub fn assess(&self, input: &ContaminationCheckInput) -> ContaminationDefenseResult {
        // Authoritative override: explicit user organization action always accepted
        if input.is_explicit_user_action {
            return ContaminationDefenseResult {
                is_contamination_risk: false,
                category: None,
                contamination_penalty: 0.0,
                is_uncorroborated_app_only: false,
                should_reject_from_context: false,
                should_hold_in_inbox: false,
                explanation: format!(
                    "Explicit user assignment into workspace '{}'.",
                    input.target_workspace_name
                ),
                reasons: vec!["Authoritative user confirmation overrides contamination filters.".to_string()],
            };
        }

        let title_lower = input.title.to_lowercase();
        let content_lower = input.content.as_deref().unwrap_or("").to_lowercase();
        let domain_norm = input
            .domain
            .as_deref()
            .map(normalize_domain)
            .or_else(|| input.url.as_deref().and_then(extract_domain_from_url));
        let app_norm = input.app_name.as_deref().unwrap_or("");

        let mut reasons = Vec::new();

        // ---------------------------------------------------------------------
        // 1. Check Entertainment / Video / Music Media
        // ---------------------------------------------------------------------
        let is_entertainment_domain = domain_norm
            .as_deref()
            .map(|d| is_domain_in_list(d, ENTERTAINMENT_DOMAINS))
            .unwrap_or(false);
        let is_entertainment_app = is_app_in_list(app_norm, ENTERTAINMENT_APPS);
        let is_youtube_in_title = title_lower.contains("youtube")
            || title_lower.contains("youtu.be")
            || title_lower.contains("netflix")
            || title_lower.contains("spotify");

        if is_entertainment_domain || is_entertainment_app || is_youtube_in_title {
            // Check if workspace is explicitly dedicated to video/music/streaming
            let ws_is_media_centric = workspace_is_dedicated_to_topic(
                &input.target_workspace_name,
                &input.target_workspace_keywords,
                &["youtube", "video", "media", "podcast", "music", "streaming", "spotify", "audio"],
            );

            if !ws_is_media_centric {
                let overlap_score = compute_topic_overlap(
                    &title_lower,
                    &content_lower,
                    &input.target_workspace_keywords,
                    &input.target_workspace_active_tasks,
                );

                if overlap_score < 0.25 && input.semantic_similarity_score < 0.35 {
                    reasons.push(format!(
                        "Entertainment activity ('{}') lacks contextual overlap with '{}'.",
                        input.title, input.target_workspace_name
                    ));
                    return ContaminationDefenseResult {
                        is_contamination_risk: true,
                        category: Some(ContaminationCategory::EntertainmentMedia),
                        contamination_penalty: ContaminationCategory::EntertainmentMedia.default_penalty(),
                        is_uncorroborated_app_only: false,
                        should_reject_from_context: true,
                        should_hold_in_inbox: true,
                        explanation: format!(
                            "Unrelated entertainment media ('{}') prevented from contaminating context '{}'.",
                            input.title, input.target_workspace_name
                        ),
                        reasons,
                    };
                }
            }
        }

        // ---------------------------------------------------------------------
        // 2. Check Social Media Activity
        // ---------------------------------------------------------------------
        let is_social_domain = domain_norm
            .as_deref()
            .map(|d| is_domain_in_list(d, SOCIAL_MEDIA_DOMAINS))
            .unwrap_or(false);
        let is_social_in_title = title_lower.contains("twitter")
            || title_lower.contains("instagram")
            || title_lower.contains("reddit")
            || title_lower.contains("facebook")
            || title_lower.contains("threads");

        if is_social_domain || is_social_in_title {
            let ws_is_social_centric = workspace_is_dedicated_to_topic(
                &input.target_workspace_name,
                &input.target_workspace_keywords,
                &["social", "twitter", "reddit", "marketing", "community", "campaign", "instagram"],
            );

            if !ws_is_social_centric {
                let overlap_score = compute_topic_overlap(
                    &title_lower,
                    &content_lower,
                    &input.target_workspace_keywords,
                    &input.target_workspace_active_tasks,
                );

                if overlap_score < 0.25 && input.semantic_similarity_score < 0.35 {
                    reasons.push(format!(
                        "Social media browsing ('{}') lacks contextual evidence for '{}'.",
                        input.title, input.target_workspace_name
                    ));
                    return ContaminationDefenseResult {
                        is_contamination_risk: true,
                        category: Some(ContaminationCategory::SocialMedia),
                        contamination_penalty: ContaminationCategory::SocialMedia.default_penalty(),
                        is_uncorroborated_app_only: false,
                        should_reject_from_context: true,
                        should_hold_in_inbox: true,
                        explanation: format!(
                            "Social media activity ('{}') prevented from contaminating context '{}'.",
                            input.title, input.target_workspace_name
                        ),
                        reasons,
                    };
                }
            }
        }

        // ---------------------------------------------------------------------
        // 3. Check Unrelated Search Queries
        // ---------------------------------------------------------------------
        let is_search_engine = domain_norm
            .as_deref()
            .map(|d| is_domain_in_list(d, SEARCH_ENGINE_DOMAINS))
            .unwrap_or(false)
            || title_lower.contains("google search")
            || title_lower.contains(" - bing")
            || title_lower.contains("duckduckgo");

        if is_search_engine {
            let search_query = extract_search_query_terms(
                &input.title,
                input.url.as_deref(),
            );

            let query_overlap = compute_topic_overlap(
                &search_query.to_lowercase(),
                &content_lower,
                &input.target_workspace_keywords,
                &input.target_workspace_active_tasks,
            );

            if query_overlap < 0.20 && input.semantic_similarity_score < 0.30 {
                reasons.push(format!(
                    "Search query '{}' does not match active tasks or topics in '{}'.",
                    search_query, input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::UnrelatedSearchQuery),
                    contamination_penalty: ContaminationCategory::UnrelatedSearchQuery.default_penalty(),
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: true,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Unrelated search query ('{}') prevented from contaminating context '{}'.",
                        search_query, input.target_workspace_name
                    ),
                    reasons,
                };
            }
        }

        // ---------------------------------------------------------------------
        // 4. Project Path & Repository Disambiguation
        // ---------------------------------------------------------------------
        let (extracted_proj_path, extracted_repo, extracted_doc) = extract_project_path_and_document(
            &input.title,
            input.url.as_deref(),
            input.file_path.as_deref(),
        );

        let ws_name_lower = input.target_workspace_name.to_lowercase();
        let ws_tokens = &input.target_workspace_keywords;

        // Check project path match or conflict
        if let Some(ref proj) = extracted_proj_path {
            let proj_lower = proj.to_lowercase();
            let matches_target_ws = ws_name_lower.contains(&proj_lower)
                || proj_lower.contains(&ws_name_lower)
                || ws_tokens.iter().any(|k| proj_lower.contains(k) || k.contains(&proj_lower));

            if matches_target_ws {
                // Strong positive match for project path!
                return ContaminationDefenseResult {
                    is_contamination_risk: false,
                    category: None,
                    contamination_penalty: 0.0,
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: false,
                    should_hold_in_inbox: false,
                    explanation: format!(
                        "Project path '{}' directly corroborates workspace '{}'.",
                        proj, input.target_workspace_name
                    ),
                    reasons: vec![format!("Project directory '{}' matches active workspace.", proj)],
                };
            } else {
                // Conflicting project path! e.g. /Projects/ContextSplit while evaluating PayX
                reasons.push(format!(
                    "Project path '{}' belongs to a different project than '{}'.",
                    proj, input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::UnrelatedFile),
                    contamination_penalty: 0.75,
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: true,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Activity from project path '{}' rejected from unrelated context '{}'.",
                        proj, input.target_workspace_name
                    ),
                    reasons,
                };
            }
        }

        // ---------------------------------------------------------------------
        // 5. Check Code Repositories (e.g. GitHub repo during Tokyo Trip or PayX)
        // ---------------------------------------------------------------------
        let is_code_repo = domain_norm
            .as_deref()
            .map(|d| d == "github.com" || d == "gitlab.com" || d == "bitbucket.org")
            .unwrap_or(false)
            || input.url.as_deref().map(|u| u.contains("github.com/") || u.contains("gitlab.com/")).unwrap_or(false)
            || extracted_repo.is_some();

        if is_code_repo {
            let ws_is_software_or_code = workspace_is_dedicated_to_topic(
                &input.target_workspace_name,
                &input.target_workspace_keywords,
                &["code", "dev", "engineering", "backend", "frontend", "api", "repo", "os", "kernel", "software", "system", "app"],
            );

            let repo_identifier = extracted_repo.or_else(|| {
                extract_repo_slug(input.url.as_deref(), &input.title)
            });

            let repo_matches_known = input.target_workspace_repos.iter().any(|r| {
                if let Some(ref slug) = repo_identifier {
                    r.to_lowercase().contains(&slug.to_lowercase())
                        || slug.to_lowercase().contains(&r.to_lowercase())
                } else {
                    false
                }
            }) || repo_identifier.as_ref().map(|slug| {
                let slug_lower = slug.to_lowercase();
                ws_name_lower.contains(&slug_lower)
                    || slug_lower.contains(&ws_name_lower)
                    || ws_tokens.iter().any(|k| slug_lower.contains(k))
            }).unwrap_or(false);

            let topic_overlap = compute_topic_overlap(
                &title_lower,
                &content_lower,
                &input.target_workspace_keywords,
                &input.target_workspace_active_tasks,
            );

            if repo_matches_known {
                // Repository matches workspace
                return ContaminationDefenseResult {
                    is_contamination_risk: false,
                    category: None,
                    contamination_penalty: 0.0,
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: false,
                    should_hold_in_inbox: false,
                    explanation: format!(
                        "Repository '{}' corroborates workspace '{}'.",
                        repo_identifier.as_deref().unwrap_or(""),
                        input.target_workspace_name
                    ),
                    reasons: vec!["Repository matches workspace configuration.".to_string()],
                };
            }

            if !ws_is_software_or_code && !repo_matches_known && topic_overlap < 0.20 {
                reasons.push(format!(
                    "Code repository activity ('{}') opened during non-coding context '{}'.",
                    repo_identifier.as_deref().unwrap_or(&input.title),
                    input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::UnrelatedRepository),
                    contamination_penalty: ContaminationCategory::UnrelatedRepository.default_penalty(),
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: true,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Unrelated repository ('{}') not attached to non-software context '{}'.",
                        repo_identifier.as_deref().unwrap_or(&input.title),
                        input.target_workspace_name
                    ),
                    reasons,
                };
            } else if !repo_matches_known && topic_overlap < 0.15 && input.semantic_similarity_score < 0.25 {
                // Workspace has known repos or is software-focused and user opened an unconfirmed/mismatched repo
                reasons.push(format!(
                    "Repository '{}' does not match confirmed repositories for '{}'.",
                    repo_identifier.as_deref().unwrap_or(&input.title),
                    input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::UnrelatedRepository),
                    contamination_penalty: 0.65,
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: true,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Unconfirmed repository ('{}') held in Inbox awaiting corroboration for '{}'.",
                        repo_identifier.as_deref().unwrap_or(&input.title),
                        input.target_workspace_name
                    ),
                    reasons,
                };
            }
        }

        // ---------------------------------------------------------------------
        // 6. Check Unrelated Local Files & Documents
        // ---------------------------------------------------------------------
        if let Some(ref path) = input.file_path {
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&input.title);
            let file_ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let file_overlap = compute_topic_overlap(
                &file_name.to_lowercase(),
                &content_lower,
                &input.target_workspace_keywords,
                &input.target_workspace_active_tasks,
            );

            let is_personal_or_tax_file = is_personal_or_financial_file(file_name, &file_ext);
            let ws_is_tax_finance = workspace_is_dedicated_to_topic(
                &input.target_workspace_name,
                &input.target_workspace_keywords,
                &["tax", "accounting", "finance", "receipt", "invoice", "payroll", "audit"],
            );

            if is_personal_or_tax_file && !ws_is_tax_finance && file_overlap < 0.20 {
                reasons.push(format!(
                    "Personal / financial document ('{}') does not belong to context '{}'.",
                    file_name, input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::UnrelatedFile),
                    contamination_penalty: ContaminationCategory::UnrelatedFile.default_penalty(),
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: true,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Unrelated document ('{}') prevented from contaminating context '{}'.",
                        file_name, input.target_workspace_name
                    ),
                    reasons,
                };
            }
        }

        // Check document name matching (e.g. Figma or Word document)
        if let Some(ref doc) = extracted_doc {
            let doc_lower = doc.to_lowercase();
            let matches_doc = ws_name_lower.contains(&doc_lower)
                || doc_lower.contains(&ws_name_lower)
                || ws_tokens.iter().any(|k| doc_lower.contains(k));
            if matches_doc {
                return ContaminationDefenseResult {
                    is_contamination_risk: false,
                    category: None,
                    contamination_penalty: 0.0,
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: false,
                    should_hold_in_inbox: false,
                    explanation: format!(
                        "Document '{}' corroborates workspace '{}'.",
                        doc, input.target_workspace_name
                    ),
                    reasons: vec![format!("Document name '{}' matches workspace topic.", doc)],
                };
            }
        }

        // ---------------------------------------------------------------------
        // 7. Check Generic Domains & Casual Shopping/News Web Browsing
        // ---------------------------------------------------------------------
        let is_generic_domain = domain_norm
            .as_deref()
            .map(|d| is_domain_in_list(d, GENERIC_SHOPPING_AND_NEWS_DOMAINS))
            .unwrap_or(false);

        if is_generic_domain {
            let ws_domain_match = domain_norm
                .as_deref()
                .map(|d| input.target_workspace_domains.iter().any(|wd| wd.contains(d) || d.contains(wd)))
                .unwrap_or(false);

            let topic_overlap = compute_topic_overlap(
                &title_lower,
                &content_lower,
                &input.target_workspace_keywords,
                &input.target_workspace_active_tasks,
            );

            if !ws_domain_match && topic_overlap < 0.20 && input.semantic_similarity_score < 0.30 {
                reasons.push(format!(
                    "Generic / shopping domain ('{}') lacks contextual evidence for '{}'.",
                    input.title, input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::GenericDomainOrShopping),
                    contamination_penalty: ContaminationCategory::GenericDomainOrShopping.default_penalty(),
                    is_uncorroborated_app_only: false,
                    should_reject_from_context: true,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Casual web browsing ('{}') prevented from contaminating context '{}'.",
                        input.title, input.target_workspace_name
                    ),
                    reasons,
                };
            }
        }

        // ---------------------------------------------------------------------
        // 8. Check Common Application Overreliance (Application Identity Alone)
        // ---------------------------------------------------------------------
        let is_common_host = is_app_in_list(app_norm, COMMON_HOST_APPS);
        if is_common_host {
            let topic_overlap = compute_topic_overlap(
                &title_lower,
                &content_lower,
                &input.target_workspace_keywords,
                &input.target_workspace_active_tasks,
            );

            let has_content_corroboration = topic_overlap >= 0.25
                || input.semantic_similarity_score >= 0.35
                || domain_norm
                    .as_deref()
                    .map(|d| input.target_workspace_domains.iter().any(|wd| wd == d))
                    .unwrap_or(false);

            if !has_content_corroboration {
                reasons.push(format!(
                    "Activity uses host app '{}' without content, domain, project path, or task corroboration for '{}'.",
                    app_norm, input.target_workspace_name
                ));
                return ContaminationDefenseResult {
                    is_contamination_risk: true,
                    category: Some(ContaminationCategory::UncorroboratedCommonApplication),
                    contamination_penalty: ContaminationCategory::UncorroboratedCommonApplication.default_penalty(),
                    is_uncorroborated_app_only: true,
                    should_reject_from_context: false,
                    should_hold_in_inbox: true,
                    explanation: format!(
                        "Context assignment requires contextual evidence, not application identity alone ('{}').",
                        app_norm
                    ),
                    reasons,
                };
            }
        }

        // Clean: No contamination detected
        ContaminationDefenseResult {
            is_contamination_risk: false,
            category: None,
            contamination_penalty: 0.0,
            is_uncorroborated_app_only: false,
            should_reject_from_context: false,
            should_hold_in_inbox: false,
            explanation: format!(
                "Activity ('{}') contains valid contextual evidence for '{}'.",
                input.title, input.target_workspace_name
            ),
            reasons: vec!["Contextual corroboration verified against workspace topics and active tasks.".to_string()],
        }
    }

    /// Evaluates contamination risk for an item in the SQLite database against a workspace.
    pub fn assess_from_db(
        &self,
        conn: &Connection,
        item_id: &str,
        target_workspace_id: &str,
    ) -> Result<ContaminationDefenseResult, String> {
        let semantic_score = crate::context_engine::scoring::evaluate_item_context(conn, item_id)
            .ok()
            .and_then(|interp| {
                interp
                    .existing_workspaces
                    .iter()
                    .find(|w| w.workspace_id == target_workspace_id)
                    .map(|w| w.score)
            })
            .unwrap_or(0.0);
        self.assess_from_db_with_semantic_score(conn, item_id, target_workspace_id, semantic_score)
    }

    /// Optimized version that reuses an already-computed semantic relevance score.
    pub fn assess_from_db_with_semantic_score(
        &self,
        conn: &Connection,
        item_id: &str,
        target_workspace_id: &str,
        semantic_score: f64,
    ) -> Result<ContaminationDefenseResult, String> {
        let item = crate::db::operations::get_item(conn, item_id).map_err(|e| e.to_string())?;
        let workspace =
            crate::db::operations::get_workspace(conn, target_workspace_id).map_err(|e| e.to_string())?;

        // 1. Check explicit user confirmation
        let is_confirmed: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM context_memberships WHERE item_id = ?1 AND workspace_id = ?2 AND membership_type = 'confirmed'",
                params![item_id, target_workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .map(|cnt| cnt > 0)
            .unwrap_or(false);

        // 2. Fetch workspace keywords, domains, repos, active tasks
        let keywords = extract_topic_tokens(&workspace.name);

        // Workspace domains from signals
        let mut domains = Vec::new();
        if let Ok(mut stmt) = conn.prepare("SELECT payload FROM context_signals WHERE workspace_id = ?1") {
            if let Ok(rows) = stmt.query_map(params![target_workspace_id], |r| r.get::<_, String>(0)) {
                for payload_str in rows.flatten() {
                    if let Ok(v) = serde_json::from_str::<Value>(&payload_str) {
                        if let Some(dom) = v.get("domain").and_then(|d| d.as_str()) {
                            domains.push(dom.to_string());
                        }
                    }
                }
            }
        }

        // Active task candidates
        let mut active_tasks = Vec::new();
        if let Ok(mut stmt) = conn.prepare("SELECT title FROM task_candidates WHERE context_id = ?1 AND status = 'active'") {
            if let Ok(rows) = stmt.query_map(params![target_workspace_id], |r| r.get::<_, String>(0)) {
                for task_name in rows.flatten() {
                    active_tasks.push(task_name);
                }
            }
        }

        // Metadata extraction
        let meta: Value = serde_json::from_str(&item.metadata).unwrap_or(Value::Null);
        let domain = meta.get("domain").and_then(|d| d.as_str()).map(|s| s.to_string());
        let url = meta.get("url").and_then(|u| u.as_str()).map(|s| s.to_string());
        let file_path = meta.get("file_path").or_else(|| meta.get("path")).and_then(|p| p.as_str()).map(|s| s.to_string());
        let app_name = meta.get("app_name").or_else(|| meta.get("application")).and_then(|a| a.as_str()).map(|s| s.to_string());

        let mut check_input = ContaminationCheckInput::new(
            item.title.unwrap_or_default(),
            target_workspace_id,
            workspace.name,
        )
        .with_semantic_score(semantic_score)
        .with_explicit_user_action(is_confirmed)
        .with_workspace_details(keywords, domains, Vec::new(), active_tasks)
        .with_content(&item.content);

        if let Some(ref d) = domain {
            check_input = check_input.with_domain(d);
        }
        if let Some(ref u) = url {
            check_input = check_input.with_url(u);
        }
        if let Some(ref p) = file_path {
            check_input = check_input.with_file_path(p);
        }
        if let Some(ref a) = app_name {
            check_input = check_input.with_app(a);
        }

        Ok(self.assess(&check_input))
    }
}

// =========================================================================
// Helper Heuristics & String Utilities
// =========================================================================

fn normalize_domain(domain: &str) -> String {
    domain
        .trim()
        .to_lowercase()
        .strip_prefix("http://")
        .unwrap_or(domain)
        .strip_prefix("https://")
        .unwrap_or(domain)
        .strip_prefix("www.")
        .unwrap_or(domain)
        .split('/')
        .next()
        .unwrap_or(domain)
        .split(':')
        .next()
        .unwrap_or(domain)
        .to_string()
}

fn extract_domain_from_url(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    let without_proto = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower);
    let domain = without_proto.split('/').next()?.split(':').next()?;
    let clean = domain.strip_prefix("www.").unwrap_or(domain);
    if clean.is_empty() {
        None
    } else {
        Some(clean.to_string())
    }
}

fn is_domain_in_list(domain: &str, list: &[&str]) -> bool {
    let norm = normalize_domain(domain);
    list.iter().any(|&item| norm == item || norm.ends_with(&format!(".{}", item)))
}

fn is_app_in_list(app: &str, list: &[&str]) -> bool {
    let app_clean = app.trim().to_lowercase();
    list.iter().any(|&item| {
        let item_clean = item.to_lowercase();
        app_clean == item_clean || app_clean.contains(&item_clean) || item_clean.contains(&app_clean)
    })
}

pub fn extract_topic_tokens(text: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "a", "an", "the", "and", "or", "but", "in", "on", "at", "to", "for", "with",
        "of", "by", "from", "up", "about", "into", "over", "after", "is", "are", "was",
        "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
        "this", "that", "these", "those", "it", "its", "my", "your", "our", "their",
        "assignment", "project", "workspace", "context", "notes", "work", "doc", "tab",
    ];

    text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.len() >= 3 && !STOPWORDS.contains(&s.as_str()))
        .collect()
}

fn compute_topic_overlap(
    title_lower: &str,
    content_lower: &str,
    workspace_keywords: &[String],
    active_tasks: &[String],
) -> f64 {
    if workspace_keywords.is_empty() && active_tasks.is_empty() {
        return 0.0;
    }

    let mut match_count = 0_usize;
    let mut total_targets = workspace_keywords.len();

    for kw in workspace_keywords {
        let kw_lower = kw.to_lowercase();
        if title_lower.contains(&kw_lower) || content_lower.contains(&kw_lower) {
            match_count += 1;
        }
    }

    for task in active_tasks {
        let task_tokens = extract_topic_tokens(task);
        for token in task_tokens {
            total_targets += 1;
            if title_lower.contains(&token) || content_lower.contains(&token) {
                match_count += 1;
            }
        }
    }

    if total_targets == 0 {
        0.0
    } else {
        (match_count as f64 / total_targets.max(1) as f64).min(1.0)
    }
}

fn workspace_is_dedicated_to_topic(
    workspace_name: &str,
    workspace_keywords: &[String],
    topic_terms: &[&str],
) -> bool {
    let name_lower = workspace_name.to_lowercase();
    for term in topic_terms {
        if name_lower.contains(term) {
            return true;
        }
        for kw in workspace_keywords {
            if kw.to_lowercase().contains(term) {
                return true;
            }
        }
    }
    false
}

pub fn extract_search_query_terms(title: &str, url: Option<&str>) -> String {
    // 1. Try URL parameters (e.g. q=..., query=...)
    if let Some(u) = url {
        if let Some(pos) = u.find("?q=").or_else(|| u.find("&q=")) {
            let sub = &u[pos + 3..];
            let raw = sub.split('&').next().unwrap_or(sub);
            let decoded = raw.replace('+', " ");
            if !decoded.trim().is_empty() {
                return decoded;
            }
        }
    }

    // 2. Try window title format: "Best gaming laptop - Google Search"
    let clean = title
        .split(" - Google Search")
        .next()
        .unwrap_or(title)
        .split(" - Bing")
        .next()
        .unwrap_or(title)
        .split(" at DuckDuckGo")
        .next()
        .unwrap_or(title)
        .split(" | DuckDuckGo")
        .next()
        .unwrap_or(title)
        .trim();

    clean.to_string()
}

pub fn extract_repo_slug(url: Option<&str>, title: &str) -> Option<String> {
    if let Some(u) = url {
        let lower = u.to_lowercase();
        if let Some(pos) = lower.find("github.com/") {
            let path = &u[pos + 11..];
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            if parts.len() >= 2 {
                return Some(format!("{}/{}", parts[0], parts[1]));
            } else if !parts.is_empty() {
                return Some(parts[0].to_string());
            }
        }
    }

    // Try title format: "torvalds/linux: Linux kernel source tree"
    if title.contains('/') {
        let candidate = title.split([' ', ':', '-']).next().unwrap_or("");
        if candidate.contains('/') && candidate.len() >= 3 {
            return Some(candidate.to_string());
        }
    }

    None
}

fn is_personal_or_financial_file(file_name: &str, file_ext: &str) -> bool {
    let lower = file_name.to_lowercase();
    const SENSITIVE_PATTERNS: &[&str] = &[
        "tax",
        "w2",
        "1099",
        "passport",
        "resume",
        "cv",
        "salary",
        "paystub",
        "receipt",
        "flight",
        "boarding_pass",
        "ticket",
        "hotel_booking",
        "medical",
        "insurance",
    ];

    for pat in SENSITIVE_PATTERNS {
        if lower.contains(pat) {
            return true;
        }
    }

    matches!(file_ext, "tax" | "qfx" | "ofx")
}

pub fn extract_project_path_and_document(
    title: &str,
    url: Option<&str>,
    file_path: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let mut project_path = None;
    let mut repo_slug = None;
    let mut document_name = None;

    const GENERIC_BUFFER_NAMES: &[&str] = &[
        "untitled",
        "untitled-1",
        "untitled-2",
        "untitled-3",
        "new tab",
        "new document",
        "document1",
        "start",
        "welcome",
        "home",
        "empty",
    ];

    // 1. Extract from file_path
    if let Some(fp) = file_path {
        let clean = fp.replace('\\', "/");
        for marker in &["/projects/", "/workspace/", "/repos/", "/code/", "/dev/", "/work/", "/src/"] {
            if let Some(pos) = clean.to_lowercase().find(marker) {
                let after = &clean[pos + marker.len()..];
                let proj = after.split('/').next().unwrap_or("").trim();
                if !proj.is_empty() {
                    project_path = Some(proj.to_string());
                    break;
                }
            }
        }
        if project_path.is_none() {
            let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() >= 2 {
                project_path = Some(parts[parts.len() - 2].to_string());
            }
        }
    }

    // 2. Extract from URL (e.g. GitHub repo, Figma file, Google Docs)
    if let Some(u) = url {
        if let Some(slug) = extract_repo_slug(Some(u), title) {
            repo_slug = Some(slug);
        }
        if u.contains("figma.com/file/") || u.contains("figma.com/design/") {
            let parts: Vec<&str> = u.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() >= 4 {
                let doc = parts[parts.len() - 1].replace('-', " ");
                if !doc.is_empty() && !GENERIC_BUFFER_NAMES.iter().any(|g| doc.to_lowercase().contains(g)) {
                    document_name = Some(doc);
                }
            }
        }
    }

    // 3. Extract from Window Title
    if project_path.is_none() {
        // e.g. "billing.rs — PayX — Visual Studio Code"
        if title.contains(" — ") {
            let parts: Vec<&str> = title.split(" — ").collect();
            if parts.len() >= 2 {
                let candidate = parts[parts.len() - 2].trim();
                let cand_lower = candidate.to_lowercase();
                if !candidate.is_empty()
                    && !candidate.contains('.')
                    && candidate.len() < 40
                    && !GENERIC_BUFFER_NAMES.iter().any(|g| cand_lower == *g || cand_lower.starts_with(g))
                {
                    project_path = Some(candidate.to_string());
                }
            }
        } else if title.contains(" - ") {
            // e.g. "PayX Mobile UI - Figma"
            let parts: Vec<&str> = title.split(" - ").collect();
            if parts.len() >= 2 {
                let candidate = parts[0].trim();
                let cand_lower = candidate.to_lowercase();
                if !candidate.is_empty() && !GENERIC_BUFFER_NAMES.iter().any(|g| cand_lower == *g) {
                    document_name = Some(candidate.to_string());
                }
            }
        }
    }

    // Check Terminal titles like "user@MacBook PayX %" or "user@MacBook:~/Projects/PayX$"
    if project_path.is_none() && (title.contains('%') || title.contains('$')) {
        let clean_title = title.split(['%', '$']).next().unwrap_or(title).trim();
        // Check part after @ if present, e.g. "MacBook PayX" or "MacBook:~/Projects/PayX"
        let host_and_dir = if let Some(pos) = clean_title.find('@') {
            &clean_title[pos + 1..]
        } else {
            clean_title
        };
        let last_word = host_and_dir
            .split([' ', ':', '/'])
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or("")
            .trim();

        if !last_word.is_empty() && last_word != "~" && last_word != "zsh" && last_word != "bash" && last_word != "MacBook" {
            project_path = Some(last_word.to_string());
        }
    }

    (project_path, repo_slug, document_name)
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_youtube_video_prevented_from_contaminating_payx_context() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new("MrBeast $10,000 Challenge - YouTube", "ws_payx", "PayX")
            .with_domain("youtube.com")
            .with_url("https://www.youtube.com/watch?v=12345")
            .with_app("Google Chrome")
            .with_workspace_details(
                vec!["payx".to_string(), "payments".to_string(), "billing".to_string()],
                vec!["payx.dev".to_string(), "stripe.com".to_string()],
                vec!["payx-backend".to_string()],
                vec!["Implement webhook verification".to_string()],
            );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::EntertainmentMedia));
        assert!(res.should_reject_from_context);
        assert!(res.should_hold_in_inbox);
        assert!(res.explanation.contains("PayX"));
    }

    #[test]
    fn test_unrelated_search_prevented_from_contaminating_university_os() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new(
            "Best gaming laptop - Google Search",
            "ws_os",
            "University OS assignment",
        )
        .with_domain("google.com")
        .with_url("https://www.google.com/search?q=Best+gaming+laptop")
        .with_app("Safari")
        .with_workspace_details(
            vec!["operating".to_string(), "system".to_string(), "kernel".to_string(), "threads".to_string()],
            vec!["canvas.edu".to_string()],
            vec!["os-xv6-assignment".to_string()],
            vec!["Implement page table walk".to_string()],
        );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::UnrelatedSearchQuery));
        assert!(res.should_reject_from_context);
        assert!(res.explanation.contains("Best gaming laptop"));
    }

    #[test]
    fn test_unrelated_github_repo_prevented_from_contaminating_tokyo_trip() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new(
            "torvalds/linux: Linux kernel source tree",
            "ws_tokyo",
            "Tokyo Trip",
        )
        .with_domain("github.com")
        .with_url("https://github.com/torvalds/linux")
        .with_app("Google Chrome")
        .with_workspace_details(
            vec!["tokyo".to_string(), "trip".to_string(), "japan".to_string(), "flights".to_string()],
            vec!["booking.com".to_string(), "airbnb.com".to_string()],
            vec![],
            vec!["Book Shinjuku hotel".to_string()],
        );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::UnrelatedRepository));
        assert!(res.should_reject_from_context);
        assert!(res.explanation.contains("Tokyo Trip"));
    }

    #[test]
    fn test_social_media_browsing_rejected_from_deep_work_context() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new(
            "Trending Tweets / X",
            "ws_fin",
            "Financial Audit 2026",
        )
        .with_domain("x.com")
        .with_url("https://x.com/explore")
        .with_app("Safari")
        .with_workspace_details(
            vec!["financial".to_string(), "audit".to_string(), "ledger".to_string()],
            vec!["quickbooks.com".to_string()],
            vec![],
            vec!["Reconcile Q3 balance".to_string()],
        );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::SocialMedia));
        assert!(res.should_reject_from_context);
    }

    #[test]
    fn test_unrelated_file_rejected_from_coding_context() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new("Tax_Return_2025.pdf", "ws_ai", "Context Engine Core")
            .with_file_path("/Users/user/Documents/Personal/Tax_Return_2025.pdf")
            .with_app("Preview")
            .with_workspace_details(
                vec!["context".to_string(), "engine".to_string(), "rust".to_string()],
                vec![],
                vec!["contextsplit".to_string()],
                vec!["Fix contradiction analyzer".to_string()],
            );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::UnrelatedFile));
        assert!(res.should_reject_from_context);
    }

    #[test]
    fn test_generic_shopping_domain_rejected_from_work_context() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new(
            "Amazon.com: Noise Cancelling Headphones",
            "ws_dev",
            "Compiler Optimization",
        )
        .with_domain("amazon.com")
        .with_url("https://www.amazon.com/dp/B012345")
        .with_app("Google Chrome")
        .with_workspace_details(
            vec!["compiler".to_string(), "llvm".to_string(), "optimization".to_string()],
            vec!["llvm.org".to_string()],
            vec!["llvm-project".to_string()],
            vec!["Implement SSA pass".to_string()],
        );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::GenericDomainOrShopping));
        assert!(res.should_reject_from_context);
    }

    #[test]
    fn test_common_app_overreliance_requires_contextual_evidence() {
        let guard = ContaminationGuard::new();
        // User opens VS Code on an empty or unrelated file without semantic or topic overlap
        let input = ContaminationCheckInput::new(
            "Untitled-1 — Visual Studio Code",
            "ws_payx",
            "PayX Billing Engine",
        )
        .with_app("Visual Studio Code")
        .with_workspace_details(
            vec!["payx".to_string(), "billing".to_string(), "stripe".to_string()],
            vec![],
            vec!["payx-billing".to_string()],
            vec!["Fix invoice rounding".to_string()],
        );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::UncorroboratedCommonApplication));
        assert!(res.is_uncorroborated_app_only);
        assert!(res.should_hold_in_inbox);
        assert!(!res.should_reject_from_context); // Held safely in inbox, not strictly rejected
        assert!(res.explanation.contains("requires contextual evidence"));
    }

    #[test]
    fn test_legitimate_corroborated_activity_passes_contamination_check() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new(
            "Stripe Webhook Signature Verification Guide",
            "ws_payx",
            "PayX Billing Engine",
        )
        .with_domain("stripe.com")
        .with_url("https://stripe.com/docs/webhooks/signatures")
        .with_app("Google Chrome")
        .with_semantic_score(0.85)
        .with_workspace_details(
            vec!["payx".to_string(), "billing".to_string(), "stripe".to_string(), "webhook".to_string()],
            vec!["stripe.com".to_string()],
            vec!["payx-billing".to_string()],
            vec!["Implement webhook verification".to_string()],
        );

        let res = guard.assess(&input);
        assert!(!res.is_contamination_risk);
        assert_eq!(res.contamination_penalty, 0.0);
        assert!(!res.should_reject_from_context);
        assert!(!res.should_hold_in_inbox);
    }

    #[test]
    fn test_vscode_open_on_payx_strongly_supports_payx() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new("billing.rs — PayX — Visual Studio Code", "ws_payx", "PayX")
            .with_app("Visual Studio Code")
            .with_file_path("/Users/user/Projects/PayX/src/billing.rs")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec![],
                vec!["payx".to_string()],
                vec!["Fix invoice rounding".to_string()],
            );

        let res = guard.assess(&input);
        assert!(!res.is_contamination_risk);
        assert_eq!(res.contamination_penalty, 0.0);
        assert!(!res.is_uncorroborated_app_only);
        assert!(!res.should_reject_from_context);
        assert!(res.explanation.contains("directly corroborates"));
    }

    #[test]
    fn test_vscode_open_on_contextsplit_supports_contextsplit_and_rejects_payx() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new(
            "main.rs — ContextSplit — Visual Studio Code",
            "ws_payx",
            "PayX",
        )
        .with_app("Visual Studio Code")
        .with_file_path("/Users/user/Projects/ContextSplit/src/main.rs")
        .with_workspace_details(
            vec!["payx".to_string(), "billing".to_string()],
            vec![],
            vec!["payx".to_string()],
            vec!["Fix invoice rounding".to_string()],
        );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::UnrelatedFile));
        assert!(res.should_reject_from_context);
        assert!(res.explanation.contains("ContextSplit"));
    }

    #[test]
    fn test_vscode_alone_without_path_is_held_in_inbox() {
        let guard = ContaminationGuard::new();
        let input = ContaminationCheckInput::new("Visual Studio Code", "ws_payx", "PayX")
            .with_app("Visual Studio Code")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec![],
                vec!["payx".to_string()],
                vec!["Fix invoice rounding".to_string()],
            );

        let res = guard.assess(&input);
        assert!(res.is_contamination_risk);
        assert_eq!(res.category, Some(ContaminationCategory::UncorroboratedCommonApplication));
        assert!(res.is_uncorroborated_app_only);
        assert!(res.should_hold_in_inbox);
        assert!(!res.should_reject_from_context);
    }

    #[test]
    fn test_figma_with_payx_design_vs_figma_alone() {
        let guard = ContaminationGuard::new();
        // 1. Figma on PayX design
        let input_payx = ContaminationCheckInput::new("PayX Mobile App UI - Figma", "ws_payx", "PayX")
            .with_app("Figma")
            .with_url("https://www.figma.com/file/12345/PayX-Mobile-App-UI")
            .with_workspace_details(
                vec!["payx".to_string(), "payments".to_string()],
                vec![],
                vec![],
                vec!["Design mobile onboarding".to_string()],
            );

        let res1 = guard.assess(&input_payx);
        assert!(!res1.is_contamination_risk);
        assert!(!res1.should_reject_from_context);

        // 2. Figma alone (Untitled)
        let input_alone = ContaminationCheckInput::new("Untitled - Figma", "ws_payx", "PayX")
            .with_app("Figma")
            .with_workspace_details(
                vec!["payx".to_string(), "payments".to_string()],
                vec![],
                vec![],
                vec!["Design mobile onboarding".to_string()],
            );

        let res2 = guard.assess(&input_alone);
        assert!(res2.is_contamination_risk);
        assert_eq!(res2.category, Some(ContaminationCategory::UncorroboratedCommonApplication));
        assert!(res2.should_hold_in_inbox);
    }

    #[test]
    fn test_terminal_with_payx_cwd_vs_terminal_alone() {
        let guard = ContaminationGuard::new();
        // 1. Terminal in ~/Projects/PayX
        let input_payx = ContaminationCheckInput::new("user@MacBook PayX %", "ws_payx", "PayX")
            .with_app("Terminal")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec![],
                vec!["payx".to_string()],
                vec!["Build backend".to_string()],
            );

        let res1 = guard.assess(&input_payx);
        assert!(!res1.is_contamination_risk);
        assert!(!res1.should_reject_from_context);

        // 2. Terminal in ~/Projects/ContextSplit while checking PayX
        let input_cs = ContaminationCheckInput::new("user@MacBook ContextSplit %", "ws_payx", "PayX")
            .with_app("Terminal")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec![],
                vec!["payx".to_string()],
                vec!["Build backend".to_string()],
            );

        let res2 = guard.assess(&input_cs);
        assert!(res2.is_contamination_risk);
        assert!(res2.should_reject_from_context);

        // 3. Terminal alone (zsh on ~)
        let input_alone = ContaminationCheckInput::new("user@MacBook ~ %", "ws_payx", "PayX")
            .with_app("Terminal")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec![],
                vec!["payx".to_string()],
                vec!["Build backend".to_string()],
            );

        let res3 = guard.assess(&input_alone);
        assert!(res3.is_contamination_risk);
        assert_eq!(res3.category, Some(ContaminationCategory::UncorroboratedCommonApplication));
        assert!(res3.should_hold_in_inbox);
    }

    #[test]
    fn test_chrome_with_url_vs_chrome_alone() {
        let guard = ContaminationGuard::new();
        // 1. Chrome on GitHub PayX repository
        let input_repo = ContaminationCheckInput::new("myorg/PayX: Payment Engine", "ws_payx", "PayX")
            .with_app("Google Chrome")
            .with_domain("github.com")
            .with_url("https://github.com/myorg/PayX")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec!["github.com".to_string()],
                vec!["myorg/payx".to_string()],
                vec!["Implement stripe webhook".to_string()],
            );

        let res1 = guard.assess(&input_repo);
        assert!(!res1.is_contamination_risk);
        assert!(!res1.should_reject_from_context);

        // 2. Chrome alone (New Tab)
        let input_alone = ContaminationCheckInput::new("New Tab", "ws_payx", "PayX")
            .with_app("Google Chrome")
            .with_workspace_details(
                vec!["payx".to_string(), "billing".to_string()],
                vec![],
                vec!["myorg/payx".to_string()],
                vec![],
            );

        let res2 = guard.assess(&input_alone);
        assert!(res2.is_contamination_risk);
        assert_eq!(res2.category, Some(ContaminationCategory::UncorroboratedCommonApplication));
        assert!(res2.should_hold_in_inbox);
    }
}
