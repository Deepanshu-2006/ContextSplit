//! Standalone Context Accuracy & False-Context Defense Offline Evaluation Binary
//!
//! Run with:
//!   cargo run --bin evaluate_accuracy

use contextsplit_lib::context_engine::evaluator::ContextAccuracyEvaluator;

fn main() {
    let (metrics, cases) = ContextAccuracyEvaluator::run_evaluation();
    let report = metrics.format_report();

    println!("{}", report);

    let failed_cases: Vec<_> = cases.iter().filter(|c| !c.passed).collect();
    if !failed_cases.is_empty() {
        eprintln!("FAILURES DETECTED ({}):", failed_cases.len());
        for f in &failed_cases {
            eprintln!("  - [{}] {}: {}", f.category, f.id, f.details);
        }
        std::process::exit(1);
    } else {
        println!("All {} evaluation cases passed successfully!", cases.len());
    }
}
