//! Real-World Context Detection Baseline Evaluation Binary
//!
//! Run with:
//!   cargo run --bin evaluate_real_world_baseline

use contextsplit_lib::context_engine::real_world_baseline::RealWorldBaselineEvaluator;

fn main() {
    let (metrics, observations) = RealWorldBaselineEvaluator::run_evaluation();
    let report = RealWorldBaselineEvaluator::format_markdown_report(&metrics, &observations);

    println!("{}", report);
}
