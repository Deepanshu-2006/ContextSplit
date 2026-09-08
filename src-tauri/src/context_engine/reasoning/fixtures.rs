use super::models::{
    CandidateContextEvidence, CuratedContextSignal, DeterministicEvidenceAssessment, EvidenceMetadata,
    EvidencePackage, ItemRelationshipEvidence, RelatedItemEvidence, SemanticSimilaritySummary,
    SessionEvidence, TargetItemEvidence, TemporalEvidence, WorkspaceProfileSummary,
};
use crate::context_engine::evidence::ContextEvidence;

/// Test fixtures for AI Context Reasoner scenarios.
/// Provides deterministic, reproducible EvidencePackage structures covering:
/// 1. Strong existing context
/// 2. Strong new context
/// 3. Ambiguous context
/// 4. No context
/// 5. Insufficient evidence

fn default_metadata(desc: &str) -> EvidenceMetadata {
    EvidenceMetadata {
        generated_at: "2026-09-04T12:00:00Z".to_string(),
        engine_version: "ContextSplit-v0.1.0".to_string(),
        total_db_items_excluded: 10,
        privacy_boundary: format!("test_fixture_{}", desc),
    }
}

fn default_temporal() -> TemporalEvidence {
    TemporalEvidence {
        age_days: 0.05,
        recency_decay_weight: 1.0,
        is_recent: true,
    }
}

/// Fixture 1: Strong Existing Context
/// Target item clearly aligns with an established workspace ("Paris Trip")
/// supported by high semantic similarity, shared session, common domain, and profile alignment.
pub fn strong_existing_context_fixture() -> EvidencePackage {
    EvidencePackage {
        target_item: TargetItemEvidence {
            id: "item_flight_cdg".to_string(),
            item_type: "text".to_string(),
            title: Some("Air France Flight to Paris CDG".to_string()),
            content_snippet: "Booking confirmation: Flight AF007 JFK to CDG departing 19:30".to_string(),
            app_name: Some("Google Chrome".to_string()),
            domain: Some("airfrance.us".to_string()),
            session_id: Some("sess_travel_booking".to_string()),
            captured_at: "2026-09-04T11:45:00Z".to_string(),
        },
        contextual_evidence: vec![
            ContextEvidence::observed_app("item_flight_cdg", "Google Chrome", Some("2026-09-04T11:45:00Z")),
            ContextEvidence::observed_domain("item_flight_cdg", "airfrance.us", Some("https://airfrance.us"), Some("2026-09-04T11:45:00Z")),
            ContextEvidence::observed_session("item_flight_cdg", "sess_travel_booking", Some("2026-09-04T11:45:00Z")),
            ContextEvidence::inferred_semantic_similarity(
                "item_flight_cdg",
                "item_hotel_paris",
                0.88,
                Some("Hotel Louvre Paris Reservation"),
                Some("Paris Trip"),
                Some("2026-09-04T11:45:00Z"),
            ),
        ],
        candidate_contexts: vec![
            CandidateContextEvidence {
                context_id: "ws_paris_trip".to_string(),
                name: "Paris Trip".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.89,
                deterministic_confidence: 0.89,
                confidence_band: "High".to_string(),
                evidence_factors: vec![
                    "High semantic similarity (88%) with 3 items in Paris Trip.".to_string(),
                    "Captured during the same activity session.".to_string(),
                    "Common domain 'airfrance.us'.".to_string(),
                ],
                item_count: 8,
                temporal_recency_score: 1.0,
            },
            CandidateContextEvidence {
                context_id: "ws_general_work".to_string(),
                name: "General Work".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.18,
                deterministic_confidence: 0.18,
                confidence_band: "Low".to_string(),
                evidence_factors: vec!["Baseline comparison with minimal contextual overlap with General Work.".to_string()],
                item_count: 24,
                temporal_recency_score: 0.85,
            },
        ],
        candidate_new_contexts: vec![],
        relevant_related_items: vec![
            RelatedItemEvidence {
                item_id: "item_hotel_paris".to_string(),
                title: Some("Hotel Louvre Paris Reservation".to_string()),
                item_type: "text".to_string(),
                snippet: "Reservation for 4 nights at Hotel Louvre".to_string(),
                workspace_id: Some("ws_paris_trip".to_string()),
                workspace_name: Some("Paris Trip".to_string()),
                relevance_score: 0.88,
            },
        ],
        relationships: vec![
            ItemRelationshipEvidence {
                other_item_id: "item_hotel_paris".to_string(),
                other_item_title: Some("Hotel Louvre Paris Reservation".to_string()),
                relationship_type: "semantic_similarity".to_string(),
                strength: 0.88,
                target_context_id: Some("ws_paris_trip".to_string()),
                explanation: "Strength 0.88 via semantic similarity".to_string(),
            },
        ],
        semantic_similarity_summaries: vec![
            SemanticSimilaritySummary {
                other_item_id: "item_hotel_paris".to_string(),
                other_item_title: Some("Hotel Louvre Paris Reservation".to_string()),
                workspace_name: Some("Paris Trip".to_string()),
                similarity_score: 0.88,
                similarity_band: "Very High".to_string(),
                summary: "Very High semantic similarity (88%) with 'Hotel Louvre Paris Reservation' in Paris Trip".to_string(),
            },
        ],
        relevant_sessions: vec![
            SessionEvidence {
                session_id: "sess_travel_booking".to_string(),
                item_count: 4,
                is_target_session: true,
                associated_workspaces: vec!["Paris Trip".to_string()],
            },
        ],
        session_context: Some(SessionEvidence {
            session_id: "sess_travel_booking".to_string(),
            item_count: 4,
            is_target_session: true,
            associated_workspaces: vec!["Paris Trip".to_string()],
        }),
        workspace_semantic_profiles: vec![
            WorkspaceProfileSummary {
                workspace_id: "ws_paris_trip".to_string(),
                workspace_name: "Paris Trip".to_string(),
                has_profile: true,
                alignment_score: Some(0.85),
                alignment_band: Some("Strong".to_string()),
                theme_summary: "Aligns strongly with the overall semantic profile of Paris Trip.".to_string(),
            },
        ],
        contextual_signals: vec![
            CuratedContextSignal {
                signal_type: "active_app".to_string(),
                summary: "Application: Google Chrome".to_string(),
            },
            CuratedContextSignal {
                signal_type: "url_metadata".to_string(),
                summary: "Domain: airfrance.us".to_string(),
            },
        ],
        temporal_context: default_temporal(),
        deterministic_assessment: DeterministicEvidenceAssessment {
            confidence_band: "high".to_string(),
            is_weak_evidence: false,
            is_ambiguous: false,
            competing_contexts: Vec::new(),
            assessment_summary: "Strong deterministic evidence for 'Paris Trip' (89% affinity).".to_string(),
        },
        system_metadata: default_metadata("strong_existing"),
    }
}

/// Fixture 2: Strong New Context
/// Target item and related items form a cohesive, novel activity cluster
/// (e.g. "Fitness & Nutrition") with no existing workspace match.
pub fn strong_new_context_fixture() -> EvidencePackage {
    let new_cand = CandidateContextEvidence {
        context_id: "cand_fitness_training".to_string(),
        name: "Fitness Training".to_string(),
        context_type: "candidate".to_string(),
        lifecycle_state: "active".to_string(),
        deterministic_score: 0.82,
        deterministic_confidence: 0.82,
        confidence_band: "High".to_string(),
        evidence_factors: vec![
            "Forms a tightly coupled semantic cluster with 4 unassigned workout items.".to_string(),
            "Shared activity session 'sess_gym_planning'.".to_string(),
            "Represents a novel topic distinct from all existing workspaces.".to_string(),
        ],
        item_count: 4,
        temporal_recency_score: 1.0,
    };

    EvidencePackage {
        target_item: TargetItemEvidence {
            id: "item_workout_routine".to_string(),
            item_type: "text".to_string(),
            title: Some("5-Day Hypertrophy Routine".to_string()),
            content_snippet: "Chest & Triceps on Monday, Back & Biceps on Tuesday, Legs on Thursday...".to_string(),
            app_name: Some("Notes".to_string()),
            domain: None,
            session_id: Some("sess_gym_planning".to_string()),
            captured_at: "2026-09-04T10:30:00Z".to_string(),
        },
        contextual_evidence: vec![
            ContextEvidence::observed_app("item_workout_routine", "Notes", Some("2026-09-04T10:30:00Z")),
            ContextEvidence::observed_session("item_workout_routine", "sess_gym_planning", Some("2026-09-04T10:30:00Z")),
            ContextEvidence::inferred_semantic_similarity(
                "item_workout_routine",
                "item_protein_shake",
                0.78,
                Some("Post-Workout Whey Protein Recipe"),
                None,
                Some("2026-09-04T10:30:00Z"),
            ),
        ],
        candidate_contexts: vec![new_cand.clone()],
        candidate_new_contexts: vec![new_cand],
        relevant_related_items: vec![
            RelatedItemEvidence {
                item_id: "item_protein_shake".to_string(),
                title: Some("Post-Workout Whey Protein Recipe".to_string()),
                item_type: "text".to_string(),
                snippet: "50g whey protein isolate, 1 banana, 300ml almond milk".to_string(),
                workspace_id: None,
                workspace_name: None,
                relevance_score: 0.78,
            },
        ],
        relationships: vec![
            ItemRelationshipEvidence {
                other_item_id: "item_protein_shake".to_string(),
                other_item_title: Some("Post-Workout Whey Protein Recipe".to_string()),
                relationship_type: "semantic_similarity".to_string(),
                strength: 0.78,
                target_context_id: None,
                explanation: "Strength 0.78 via semantic similarity".to_string(),
            },
        ],
        semantic_similarity_summaries: vec![
            SemanticSimilaritySummary {
                other_item_id: "item_protein_shake".to_string(),
                other_item_title: Some("Post-Workout Whey Protein Recipe".to_string()),
                workspace_name: None,
                similarity_score: 0.78,
                similarity_band: "High".to_string(),
                summary: "High semantic similarity (78%) with 'Post-Workout Whey Protein Recipe' in Inbox".to_string(),
            },
        ],
        relevant_sessions: vec![
            SessionEvidence {
                session_id: "sess_gym_planning".to_string(),
                item_count: 3,
                is_target_session: true,
                associated_workspaces: vec![],
            },
        ],
        session_context: Some(SessionEvidence {
            session_id: "sess_gym_planning".to_string(),
            item_count: 3,
            is_target_session: true,
            associated_workspaces: vec![],
        }),
        workspace_semantic_profiles: vec![],
        contextual_signals: vec![
            CuratedContextSignal {
                signal_type: "active_app".to_string(),
                summary: "Application: Notes".to_string(),
            },
        ],
        temporal_context: default_temporal(),
        deterministic_assessment: DeterministicEvidenceAssessment {
            confidence_band: "high".to_string(),
            is_weak_evidence: false,
            is_ambiguous: false,
            competing_contexts: Vec::new(),
            assessment_summary: "Strong deterministic evidence for new context cluster 'Workout Plan' (82% affinity).".to_string(),
        },
        system_metadata: default_metadata("strong_new"),
    }
}

/// Fixture 3: Ambiguous Context
/// Target item exhibits competing, balanced affinity between two active workspaces
/// ("Paris Vacation" and "London Client Meetings"), e.g. a Eurostar booking between both cities.
pub fn ambiguous_context_fixture() -> EvidencePackage {
    EvidencePackage {
        target_item: TargetItemEvidence {
            id: "item_eurostar_ticket".to_string(),
            item_type: "text".to_string(),
            title: Some("Eurostar Ticket London St Pancras to Paris Gare du Nord".to_string()),
            content_snippet: "Standard Premier ticket departing London 09:01 arriving Paris 12:17".to_string(),
            app_name: Some("Google Chrome".to_string()),
            domain: Some("eurostar.com".to_string()),
            session_id: Some("sess_cross_channel".to_string()),
            captured_at: "2026-09-04T09:15:00Z".to_string(),
        },
        contextual_evidence: vec![
            ContextEvidence::observed_app("item_eurostar_ticket", "Google Chrome", Some("2026-09-04T09:15:00Z")),
            ContextEvidence::observed_domain("item_eurostar_ticket", "eurostar.com", Some("https://eurostar.com"), Some("2026-09-04T09:15:00Z")),
            ContextEvidence::observed_session("item_eurostar_ticket", "sess_cross_channel", Some("2026-09-04T09:15:00Z")),
        ],
        candidate_contexts: vec![
            CandidateContextEvidence {
                context_id: "ws_paris_trip".to_string(),
                name: "Paris Trip".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.63,
                deterministic_confidence: 0.63,
                confidence_band: "High".to_string(),
                evidence_factors: vec![
                    "Shared domain 'eurostar.com'.".to_string(),
                    "Arrival destination matches Paris Trip.".to_string(),
                ],
                item_count: 6,
                temporal_recency_score: 1.0,
            },
            CandidateContextEvidence {
                context_id: "ws_london_meetings".to_string(),
                name: "London Client Meetings".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.59,
                deterministic_confidence: 0.59,
                confidence_band: "Medium".to_string(),
                evidence_factors: vec![
                    "Departure location matches London Client Meetings.".to_string(),
                    "Shared domain 'eurostar.com'.".to_string(),
                ],
                item_count: 10,
                temporal_recency_score: 1.0,
            },
        ],
        candidate_new_contexts: vec![],
        relevant_related_items: vec![
            RelatedItemEvidence {
                item_id: "item_london_hotel".to_string(),
                title: Some("London Hotel Booking".to_string()),
                item_type: "text".to_string(),
                snippet: "Kings Cross hotel confirmation".to_string(),
                workspace_id: Some("ws_london_meetings".to_string()),
                workspace_name: Some("London Client Meetings".to_string()),
                relevance_score: 0.60,
            },
            RelatedItemEvidence {
                item_id: "item_paris_hotel".to_string(),
                title: Some("Paris Hotel Booking".to_string()),
                item_type: "text".to_string(),
                snippet: "Hotel reservation in Paris".to_string(),
                workspace_id: Some("ws_paris_trip".to_string()),
                workspace_name: Some("Paris Trip".to_string()),
                relevance_score: 0.62,
            },
        ],
        relationships: vec![
            ItemRelationshipEvidence {
                other_item_id: "item_paris_hotel".to_string(),
                other_item_title: Some("Paris Hotel Booking".to_string()),
                relationship_type: "semantic_similarity".to_string(),
                strength: 0.62,
                target_context_id: Some("ws_paris_trip".to_string()),
                explanation: "Strength 0.62 via semantic similarity".to_string(),
            },
            ItemRelationshipEvidence {
                other_item_id: "item_london_hotel".to_string(),
                other_item_title: Some("London Hotel Booking".to_string()),
                relationship_type: "semantic_similarity".to_string(),
                strength: 0.60,
                target_context_id: Some("ws_london_meetings".to_string()),
                explanation: "Strength 0.60 via semantic similarity".to_string(),
            },
        ],
        semantic_similarity_summaries: vec![
            SemanticSimilaritySummary {
                other_item_id: "item_paris_hotel".to_string(),
                other_item_title: Some("Paris Hotel Booking".to_string()),
                workspace_name: Some("Paris Trip".to_string()),
                similarity_score: 0.62,
                similarity_band: "High".to_string(),
                summary: "High semantic similarity (62%) with 'Paris Hotel Booking' in Paris Trip".to_string(),
            },
            SemanticSimilaritySummary {
                other_item_id: "item_london_hotel".to_string(),
                other_item_title: Some("London Hotel Booking".to_string()),
                workspace_name: Some("London Client Meetings".to_string()),
                similarity_score: 0.60,
                similarity_band: "High".to_string(),
                summary: "High semantic similarity (60%) with 'London Hotel Booking' in London Client Meetings".to_string(),
            },
        ],
        relevant_sessions: vec![],
        session_context: None,
        workspace_semantic_profiles: vec![],
        contextual_signals: vec![
            CuratedContextSignal {
                signal_type: "url_metadata".to_string(),
                summary: "Domain: eurostar.com".to_string(),
            },
        ],
        temporal_context: default_temporal(),
        deterministic_assessment: DeterministicEvidenceAssessment {
            confidence_band: "uncertain".to_string(),
            is_weak_evidence: false,
            is_ambiguous: true,
            competing_contexts: vec!["Paris Trip".to_string(), "London Client Meetings".to_string()],
            assessment_summary: "Ambiguous signals: competing affinity between 'Paris Trip' (63%) and 'London Client Meetings' (59%).".to_string(),
        },
        system_metadata: default_metadata("ambiguous"),
    }
}

/// Fixture 4: No Context
/// Item has zero candidate contexts in the database (or only explicitly archived contexts).
/// The reasoner must cleanly identify that no context exists and recommend keeping in Inbox.
pub fn no_context_fixture() -> EvidencePackage {
    EvidencePackage {
        target_item: TargetItemEvidence {
            id: "item_isolated_note".to_string(),
            item_type: "text".to_string(),
            title: Some("Random Thought".to_string()),
            content_snippet: "Pick up milk and sourdough bread from the bakery on 4th street".to_string(),
            app_name: Some("Notes".to_string()),
            domain: None,
            session_id: None,
            captured_at: "2026-09-04T08:00:00Z".to_string(),
        },
        contextual_evidence: vec![
            ContextEvidence::observed_app("item_isolated_note", "Notes", Some("2026-09-04T08:00:00Z")),
            ContextEvidence::observed_content_type("item_isolated_note", "text", Some("2026-09-04T08:00:00Z")),
        ],
        candidate_contexts: vec![],
        candidate_new_contexts: vec![],
        relevant_related_items: vec![],
        relationships: vec![],
        semantic_similarity_summaries: vec![],
        relevant_sessions: vec![],
        session_context: None,
        workspace_semantic_profiles: vec![],
        contextual_signals: vec![
            CuratedContextSignal {
                signal_type: "active_app".to_string(),
                summary: "Application: Notes".to_string(),
            },
        ],
        temporal_context: default_temporal(),
        deterministic_assessment: DeterministicEvidenceAssessment {
            confidence_band: "none".to_string(),
            is_weak_evidence: true,
            is_ambiguous: false,
            competing_contexts: Vec::new(),
            assessment_summary: "No candidate contexts or matching signals were detected for this item.".to_string(),
        },
        system_metadata: default_metadata("no_context"),
    }
}

/// Fixture 5: Insufficient Evidence
/// Candidate contexts exist, but their deterministic affinity scores and relationship strengths
/// fall far below the actionable threshold (< 0.25).
/// The reasoner must reject association and return NoContext with an explicit uncertainty_reason.
pub fn insufficient_evidence_fixture() -> EvidencePackage {
    EvidencePackage {
        target_item: TargetItemEvidence {
            id: "item_vague_snippet".to_string(),
            item_type: "text".to_string(),
            title: Some("Snippet".to_string()),
            content_snippet: "meeting at 3pm".to_string(),
            app_name: Some("Calendar".to_string()),
            domain: None,
            session_id: None,
            captured_at: "2026-09-04T07:30:00Z".to_string(),
        },
        contextual_evidence: vec![
            ContextEvidence::observed_app("item_vague_snippet", "Calendar", Some("2026-09-04T07:30:00Z")),
            ContextEvidence::observed_content_type("item_vague_snippet", "text", Some("2026-09-04T07:30:00Z")),
        ],
        candidate_contexts: vec![
            CandidateContextEvidence {
                context_id: "ws_quarterly_planning".to_string(),
                name: "Quarterly Planning".to_string(),
                context_type: "workspace".to_string(),
                lifecycle_state: "active".to_string(),
                deterministic_score: 0.14,
                deterministic_confidence: 0.14,
                confidence_band: "Low".to_string(),
                evidence_factors: vec![
                    "Common word 'meeting' with minimal relevance.".to_string(),
                ],
                item_count: 15,
                temporal_recency_score: 0.5,
            },
        ],
        candidate_new_contexts: vec![],
        relevant_related_items: vec![],
        relationships: vec![],
        semantic_similarity_summaries: vec![],
        relevant_sessions: vec![],
        session_context: None,
        workspace_semantic_profiles: vec![
            WorkspaceProfileSummary {
                workspace_id: "ws_quarterly_planning".to_string(),
                workspace_name: "Quarterly Planning".to_string(),
                has_profile: true,
                alignment_score: Some(0.12),
                alignment_band: Some("Weak".to_string()),
                theme_summary: "Differs from the primary conceptual focus of Quarterly Planning.".to_string(),
            },
        ],
        contextual_signals: vec![
            CuratedContextSignal {
                signal_type: "active_app".to_string(),
                summary: "Application: Calendar".to_string(),
            },
        ],
        temporal_context: default_temporal(),
        deterministic_assessment: DeterministicEvidenceAssessment {
            confidence_band: "weak".to_string(),
            is_weak_evidence: true,
            is_ambiguous: false,
            competing_contexts: Vec::new(),
            assessment_summary: "Deterministic evidence for 'Quarterly Planning' is weak (14% affinity is below the actionable 25% threshold).".to_string(),
        },
        system_metadata: default_metadata("insufficient_evidence"),
    }
}
