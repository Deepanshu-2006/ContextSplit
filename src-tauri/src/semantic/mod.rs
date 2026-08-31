use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub struct Embedding(pub Vec<f32>);

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub is_ready: bool,
    pub model_name: String,
    pub error: Option<String>,
}

pub trait SemanticProvider: Send + Sync {
    /// Generates an embedding vector for the given text.
    fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, String>;
    
    /// Calculates the semantic similarity between two embeddings.
    /// Default implementation uses cosine similarity.
    fn semantic_similarity(&self, emb1: &[f32], emb2: &[f32]) -> f32 {
        cosine_similarity(emb1, emb2)
    }
    
    /// Returns the health/status of the provider.
    fn status(&self) -> ProviderStatus;
}

pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
    if v1.is_empty() || v2.is_empty() || v1.len() != v2.len() {
        return 0.0;
    }
    
    let mut dot_product = 0.0;
    let mut norm1 = 0.0;
    let mut norm2 = 0.0;
    
    for (a, b) in v1.iter().zip(v2.iter()) {
        dot_product += a * b;
        norm1 += a * a;
        norm2 += b * b;
    }
    
    if norm1 == 0.0 || norm2 == 0.0 {
        return 0.0;
    }
    
    dot_product / (norm1.sqrt() * norm2.sqrt())
}

/// A mock provider for testing purposes that generates deterministic pseudo-embeddings.
pub struct MockSemanticProvider {
    pub model_name: String,
}

impl MockSemanticProvider {
    pub fn new() -> Self {
        Self {
            model_name: "mock-semantic-v1".to_string(),
        }
    }
}

impl SemanticProvider for MockSemanticProvider {
    fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, String> {
        // Deterministic pseudo-embedding for testing
        // We'll generate a vector of size 3 based on text length and some basic hashing
        let len = text.len() as f32;
        let v1 = len / 100.0;
        let v2 = if text.contains("test") { 1.0 } else { 0.0 };
        let v3 = text.chars().filter(|c| c.is_alphabetic()).count() as f32 / 50.0;
        
        // Normalize the vector so cosine similarity works as expected
        let norm = (v1*v1 + v2*v2 + v3*v3).sqrt();
        let embedding = if norm > 0.0 {
            vec![v1/norm, v2/norm, v3/norm]
        } else {
            vec![0.0, 0.0, 0.0]
        };
        
        Ok(embedding)
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus {
            is_ready: true,
            model_name: self.model_name.clone(),
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v2) - 1.0).abs() < f32::EPSILON);

        let v3 = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&v1, &v3) - 0.0).abs() < f32::EPSILON);
    }
    
    #[test]
    fn test_mock_provider() {
        let provider = MockSemanticProvider::new();
        let emb1 = provider.generate_embedding("this is a test string").unwrap();
        let emb2 = provider.generate_embedding("another test string").unwrap();
        
        assert_eq!(emb1.len(), 3);
        let sim = provider.semantic_similarity(&emb1, &emb2);
        assert!(sim >= 0.0 && sim <= 1.0);
    }
}
