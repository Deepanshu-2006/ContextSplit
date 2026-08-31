use crate::db::models::Item;
use regex::Regex;
use serde_json::Value;

pub struct SemanticContentBuilder {
    pub max_text_length: usize,
}

impl Default for SemanticContentBuilder {
    fn default() -> Self {
        Self {
            max_text_length: 4000,
        }
    }
}

impl SemanticContentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(&self, item: &Item) -> String {
        let mut parts = Vec::new();

        // 1. Title is always relevant if it exists
        if let Some(title) = &item.title {
            if !title.trim().is_empty() {
                parts.push(format!("Title: {}", title.trim()));
            }
        }

        // 2. Type-specific content extraction
        match item.item_type.as_str() {
            "text" => {
                let content = if item.content.len() > self.max_text_length {
                    let mut truncated = String::from(&item.content[..self.max_text_length]);
                    truncated.push_str("...[TRUNCATED]");
                    truncated
                } else {
                    item.content.clone()
                };
                parts.push(format!("Content: {}", content));
            }
            "url" => {
                // Try to extract clean URL information, ignoring full HTML body
                parts.push(format!("URL: {}", item.content.trim()));
            }
            "file" => {
                // For files, we only want metadata like filename and MIME type
                parts.push(String::from("Type: File"));
            }
            "image" => {
                // Never embed binary image data
                parts.push(String::from("Type: Image"));
            }
            _ => {
                parts.push(format!("Type: {}", item.item_type));
            }
        }

        // 3. Metadata extraction
        if let Ok(meta_val) = serde_json::from_str::<Value>(&item.metadata) {
            if let Some(obj) = meta_val.as_object() {
                // Include app name if available
                if let Some(app_name) = obj.get("app_name").and_then(|v| v.as_str()) {
                    parts.push(format!("App: {}", app_name));
                }
                // Include domain if available
                if let Some(domain) = obj.get("domain").and_then(|v| v.as_str()) {
                    parts.push(format!("Domain: {}", domain));
                }
                // File specifics
                if let Some(filename) = obj.get("filename").and_then(|v| v.as_str()) {
                    parts.push(format!("Filename: {}", filename));
                }
                if let Some(mime) = obj.get("mime_type").and_then(|v| v.as_str()) {
                    parts.push(format!("MIME: {}", mime));
                }
            }
        }

        let raw_string = parts.join("\n");
        let normalized = self.normalize_whitespace(&raw_string);
        self.scrub_secrets(&normalized)
    }

    fn normalize_whitespace(&self, input: &str) -> String {
        // Replace multiple spaces/newlines with a single space
        let re = Regex::new(r"\s+").unwrap();
        re.replace_all(input, " ").to_string().trim().to_string()
    }

    fn scrub_secrets(&self, input: &str) -> String {
        let mut safe_input = input.to_string();

        // Regex patterns for obvious secrets
        let patterns = [
            (Regex::new(r"Bearer\s+[A-Za-z0-9\-\._~+/]+").unwrap(), "Bearer [REDACTED]"),
            (Regex::new(r"ey[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+").unwrap(), "[JWT_REDACTED]"),
            (Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(), "[AWS_KEY_REDACTED]"),
        ];

        for (re, replacement) in patterns.iter() {
            safe_input = re.replace_all(&safe_input, *replacement).to_string();
        }

        safe_input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_item(item_type: &str, content: &str, title: Option<&str>, metadata: &str) -> Item {
        Item {
            id: "test-id".to_string(),
            workspace_id: None,
            session_id: None,
            item_type: item_type.to_string(),
            content: content.to_string(),
            title: title.map(|s| s.to_string()),
            metadata: metadata.to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn test_text_item_formatting() {
        let builder = SemanticContentBuilder::new();
        let item = create_test_item("text", "This is some content\nwith newlines.", Some("My Title"), r#"{"app_name": "Notes"}"#);
        
        let result = builder.build(&item);
        
        assert!(result.contains("Title: My Title"));
        assert!(result.contains("Content: This is some content with newlines."));
        assert!(result.contains("App: Notes"));
        assert!(!result.contains("\n")); // whitespace normalized
    }

    #[test]
    fn test_text_truncation() {
        let builder = SemanticContentBuilder { max_text_length: 10 };
        let item = create_test_item("text", "123456789012345", None, "{}");
        
        let result = builder.build(&item);
        
        assert!(result.contains("Content: 1234567890...[TRUNCATED]"));
    }

    #[test]
    fn test_secret_scrubbing() {
        let builder = SemanticContentBuilder::new();
        
        // JWT
        let item1 = create_test_item("text", "Token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c", None, "{}");
        let result1 = builder.build(&item1);
        assert!(!result1.contains("eyJhbGci"));
        assert!(result1.contains("Token: [JWT_REDACTED]"));

        // Bearer
        let item2 = create_test_item("text", "Auth: Bearer 1234567890abcdef", None, "{}");
        let result2 = builder.build(&item2);
        assert!(!result2.contains("1234567890abcdef"));
        assert!(result2.contains("Auth: Bearer [REDACTED]"));

        // AWS
        let item3 = create_test_item("text", "Key: AKIAIOSFODNN7EXAMPLE", None, "{}");
        let result3 = builder.build(&item3);
        assert!(!result3.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(result3.contains("Key: [AWS_KEY_REDACTED]"));
    }

    #[test]
    fn test_file_metadata_only() {
        let builder = SemanticContentBuilder::new();
        let item = create_test_item("file", "base64_or_binary_data_here", Some("My File"), r#"{"filename": "document.pdf", "mime_type": "application/pdf"}"#);
        
        let result = builder.build(&item);
        
        assert!(result.contains("Title: My File"));
        assert!(result.contains("Type: File"));
        assert!(result.contains("Filename: document.pdf"));
        assert!(result.contains("MIME: application/pdf"));
        assert!(!result.contains("base64_or_binary_data_here")); // Content should be ignored for files
    }
}
