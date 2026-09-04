use rusqlite::Connection;
use tauri::AppHandle;

use super::models::{CaptureError, CaptureEvent, CaptureResult, RawCapturePayload};
use super::normalizer::CaptureNormalizer;
use super::pipeline::CapturePipeline;
use super::validator::CaptureValidator;
use crate::context_engine::engine::ContextEngine;

/// The central platform-independent capture engine.
/// Provides a unified interface over the 9-step `CapturePipeline`.
#[derive(Debug, Clone, Default)]
pub struct CaptureCore {
    pipeline: CapturePipeline,
}

impl CaptureCore {
    pub fn new() -> Self {
        Self {
            pipeline: CapturePipeline::new(),
        }
    }

    pub fn with_validator(mut self, validator: CaptureValidator) -> Self {
        self.pipeline = self.pipeline.with_validator(validator);
        self
    }

    pub fn with_normalizer(mut self, normalizer: CaptureNormalizer) -> Self {
        self.pipeline = self.pipeline.with_normalizer(normalizer);
        self
    }

    pub fn with_dedup_window_secs(mut self, secs: u64) -> Self {
        self.pipeline = self.pipeline.with_dedup_window_secs(secs);
        self
    }

    pub fn without_deduplication(mut self) -> Self {
        self.pipeline = self.pipeline.without_deduplication();
        self
    }

    pub fn pipeline(&self) -> &CapturePipeline {
        &self.pipeline
    }

    /// Step 1: Validate a raw payload
    pub fn validate(&self, raw: &RawCapturePayload) -> Result<(), CaptureError> {
        let validator = CaptureValidator::new();
        validator.validate(raw)
    }

    /// Step 2: Normalize a raw payload into a structured CaptureEvent
    pub fn normalize(&self, raw: &RawCapturePayload) -> Result<CaptureEvent, CaptureError> {
        let normalizer = CaptureNormalizer::new();
        let event = normalizer.normalize(raw)?;
        event.validate()?;
        Ok(event)
    }

    /// Executes the full 9-step capture pipeline from a CaptureEvent in headless mode.
    pub fn process_event_headless(
        &self,
        conn: &Connection,
        context_engine: Option<&ContextEngine>,
        event: CaptureEvent,
    ) -> Result<CaptureResult, CaptureError> {
        self.pipeline.process_event_headless(conn, context_engine, event)
    }

    /// Executes the full 9-step capture pipeline from a RawCapturePayload in headless mode.
    pub fn process_headless(
        &self,
        conn: &Connection,
        context_engine: Option<&ContextEngine>,
        raw: RawCapturePayload,
    ) -> Result<CaptureResult, CaptureError> {
        self.pipeline.process_raw_headless(conn, context_engine, raw)
    }

    /// Executes the full 9-step capture pipeline from a CaptureEvent in live Tauri app mode.
    pub fn process_event_app(
        &self,
        app_handle: AppHandle,
        event: CaptureEvent,
    ) -> Result<CaptureResult, CaptureError> {
        self.pipeline.process_event_app(app_handle, event)
    }

    /// Executes the full 9-step capture pipeline from a RawCapturePayload in live Tauri app mode.
    pub fn process_app(
        &self,
        app_handle: AppHandle,
        raw: RawCapturePayload,
    ) -> Result<CaptureResult, CaptureError> {
        self.pipeline.process_raw_app(app_handle, raw)
    }
}
