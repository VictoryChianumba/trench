use crate::Note;
use rayon::prelude::*;

pub mod criterion;

pub use criterion::FilterCriterion;

#[derive(Debug, Clone, Copy)]
pub enum CriteriaRelation {
    And,
    Or,
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub relation: CriteriaRelation,
    pub criteria: Vec<FilterCriterion>,
}

impl Default for Filter {
    fn default() -> Self {
        Filter {
            relation: CriteriaRelation::And,
            criteria: Vec::new(),
        }
    }
}

impl Filter {
    /// Checks if the note meets the filter criteria
    pub fn check_note(&self, note: &Note) -> bool {
        match self.relation {
            CriteriaRelation::And => self.criteria.par_iter().all(|cr| cr.check_note(note)),
            CriteriaRelation::Or => self.criteria.par_iter().any(|cr| cr.check_note(note)),
        }
    }
}
