use super::{SemanticProvider, ProviderStatus};
use std::sync::Mutex;
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

pub struct FastEmbedProvider {
    model: Mutex<TextEmbedding>,
    model_name: String,
}

impl FastEmbedProvider {
    pub fn try_new() -> Result<Self, String> {
        let model_name = "bge-small-en-v1.5";
        
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15)
                .with_show_download_progress(false)
        ).map_err(|e| format!("Failed to initialize FastEmbed: {}", e))?;
        
        Ok(Self {
            model: Mutex::new(model),
            model_name: format!("fastembed-{}", model_name),
        })
    }
}

impl SemanticProvider for FastEmbedProvider {
    fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, String> {
        // Embed the single text snippet
        let embeddings = self.model
            .lock()
            .map_err(|_| "Failed to acquire lock on model".to_string())?
            .embed(vec![text.to_string()], None)
            .map_err(|e| format!("Embedding generation failed: {}", e))?;
            
        if let Some(first_embedding) = embeddings.into_iter().next() {
            Ok(first_embedding)
        } else {
            Err("Model returned no embeddings".to_string())
        }
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
    #[ignore] // Ignore in CI by default as it downloads an ~80MB model
    fn test_fastembed_generation() {
        let provider = FastEmbedProvider::try_new().expect("Failed to create FastEmbedProvider");
        
        let status = provider.status();
        assert!(status.is_ready);
        assert_eq!(status.model_name, "fastembed-bge-small-en-v1.5");
        
        let embedding = provider.generate_embedding("Hello world").expect("Failed to generate embedding");
        
        // bge-small-en-v1.5 produces a 384-dimensional vector
        assert_eq!(embedding.len(), 384);
    }
}
