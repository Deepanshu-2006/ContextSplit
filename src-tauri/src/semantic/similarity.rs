use lru::LruCache;
use std::num::NonZeroUsize;
use rusqlite::Connection;
use crate::db::operations::get_item_embedding;
use super::cosine_similarity;

pub struct SemanticSimilarityService {
    cache: LruCache<(String, String), f32>,
}

impl SemanticSimilarityService {
    pub fn new(cache_size: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(1000).unwrap())),
        }
    }

    /// Calculates the semantic similarity between two items.
    /// Returns None if either item is missing an embedding, or if the embeddings are from incompatible models.
    pub fn calculate_similarity(
        &mut self,
        conn: &Connection,
        item1_id: &str,
        item2_id: &str,
    ) -> Option<f32> {
        // Deterministic cache key (lexicographically sorted)
        let key = if item1_id < item2_id {
            (item1_id.to_string(), item2_id.to_string())
        } else {
            (item2_id.to_string(), item1_id.to_string())
        };

        // Check cache to avoid unnecessary repeated calculations
        if let Some(&score) = self.cache.get(&key) {
            return Some(score);
        }

        // Fetch embeddings from DB
        let emb1_opt = get_item_embedding(conn, item1_id).unwrap_or(None);
        let emb2_opt = get_item_embedding(conn, item2_id).unwrap_or(None);

        if let (Some(emb1), Some(emb2)) = (emb1_opt, emb2_opt) {
            // Ensure models and providers are compatible
            if emb1.provider != emb2.provider || emb1.model != emb2.model {
                return None;
            }

            let score = cosine_similarity(&emb1.embedding, &emb2.embedding);
            self.cache.put(key, score);
            Some(score)
        } else {
            // One or both items are missing an embedding
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_semantic_similarity_service() {
        let conn = setup_db();
        let mut service = SemanticSimilarityService::new(10);
        
        // Create items to satisfy FOREIGN KEY constraints
        db::operations::create_item(&conn, None, None, "text", "test1", None, "{}").unwrap();
        db::operations::create_item(&conn, None, None, "text", "test2", None, "{}").unwrap();
        db::operations::create_item(&conn, None, None, "text", "test3", None, "{}").unwrap();
        
        // Since create_item generates a random UUID, we need to get the items back
        let items = db::operations::list_items(&conn, None).unwrap();
        let item1_id = &items[0].id;
        let item2_id = &items[1].id;
        let item3_id = &items[2].id;

        // Save two identical embeddings
        let emb_a = vec![1.0, 0.0, 0.0];
        let emb_b = vec![1.0, 0.0, 0.0];
        db::operations::save_item_embedding(&conn, item1_id, &emb_a, "provider-a", "model-1").unwrap();
        db::operations::save_item_embedding(&conn, item2_id, &emb_b, "provider-a", "model-1").unwrap();

        let score = service.calculate_similarity(&conn, item1_id, item2_id);
        assert_eq!(score, Some(1.0));

        // Test caching (the cache should now contain this tuple, properly sorted)
        let key = if item1_id < item2_id {
            (item1_id.clone(), item2_id.clone())
        } else {
            (item2_id.clone(), item1_id.clone())
        };
        assert_eq!(service.cache.get(&key), Some(&1.0));

        // Test incompatible models
        db::operations::save_item_embedding(&conn, item3_id, &emb_b, "provider-a", "model-2").unwrap();
        let score_incompatible = service.calculate_similarity(&conn, item1_id, item3_id);
        assert_eq!(score_incompatible, None);

        // Test missing embedding
        let score_missing = service.calculate_similarity(&conn, item1_id, "non-existent-item");
        assert_eq!(score_missing, None);
    }
}
