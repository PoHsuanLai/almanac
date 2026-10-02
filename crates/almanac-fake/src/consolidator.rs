//! A consolidator that answers from a script.

use almanac_service::{ConsolidateError, ConsolidationInput, Consolidator, Draft};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Answers each `draft` with the next scripted result; an empty script answers with an empty
/// draft. Records every input it was given.
#[derive(Debug, Default)]
pub struct ScriptedConsolidator {
    script: Mutex<VecDeque<Result<Draft, ConsolidateError>>>,
    seen: Mutex<Vec<ConsolidationInput>>,
}

impl ScriptedConsolidator {
    /// Answers with these results, in order.
    pub fn answering(script: impl IntoIterator<Item = Result<Draft, ConsolidateError>>) -> Self {
        Self {
            script: Mutex::new(script.into_iter().collect()),
            seen: Mutex::default(),
        }
    }

    /// The inputs `draft` was called with so far.
    pub fn inputs(&self) -> Vec<ConsolidationInput> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Consolidator for ScriptedConsolidator {
    async fn draft(&self, input: ConsolidationInput) -> Result<Draft, ConsolidateError> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(input);
        self.script
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
            .unwrap_or(Ok(Draft { hunks: Vec::new() }))
    }
}
