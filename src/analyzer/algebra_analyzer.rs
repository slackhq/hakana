use std::{collections::HashSet, rc::Rc};

use hakana_algebra::Clause;

use hakana_code_info::{
    assertion::Assertion,
    functionlike_identifier::FunctionLikeIdentifier,
    issue::{Issue, IssueKind},
};
use hakana_parser::aast::Pos;
use rustc_hash::FxHashSet;

use crate::{
    function_analysis_data::FunctionAnalysisData, scope_analyzer::ScopeAnalyzer,
    statements_analyzer::StatementsAnalyzer,
};

pub(crate) fn check_for_paradox(
    statements_analyzer: &StatementsAnalyzer,
    formula_1: &Vec<Rc<Clause>>,
    formula_2: &Vec<Clause>,
    analysis_data: &mut FunctionAnalysisData,
    pos: &Pos,
    calling_functionlike_id: &Option<FunctionLikeIdentifier>,
) {
    let negated_formula_2 = hakana_algebra::negate_formula(formula_2.to_vec());

    if negated_formula_2.is_err() {
        return;
    }

    let negated_formula_2 = negated_formula_2.unwrap();

    let formula_1_hashes: FxHashSet<&Clause> = HashSet::from_iter(formula_1.iter().map(|v| &**v));

    let mut formula_2_hashes = FxHashSet::default();

    for formula_2_clause in formula_2 {
        if !formula_2_clause.generated
            && !formula_2_clause.wedge
            && formula_2_clause.reconcilable
            && (formula_1_hashes.contains(formula_2_clause)
                || formula_2_hashes.contains(formula_2_clause))
            && !formula_2_clause.possibilities.iter().any(|(_, c)| {
                c.iter().any(|c| {
                    matches!(
                        c.1,
                        Assertion::DontIgnoreTaints | Assertion::DontRemoveTaints(..),
                    )
                })
            })
        {
            let clause_string = formula_2_clause.to_string(statements_analyzer.interner);

            analysis_data.maybe_add_issue(
                Issue::new(
                    if clause_string == "isset" {
                        IssueKind::RedundantIssetCheck
                    } else {
                        IssueKind::RedundantTypeComparison
                    },
                    format!("{} {}", clause_string, "has already been asserted"),
                    statements_analyzer.get_hpos(pos),
                    calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                &statements_analyzer
                    .file_analyzer
                    .file_source
                    .file_path_actual,
            );
        }

        formula_2_hashes.insert(formula_2_clause);
    }

    // Each clause of the negated condition is the negation of one way the condition can hold,
    // so a clause that follows from what is already known rules that way out.
    let contradicted_clauses = negated_formula_2
        .iter()
        .map(|negated_clause_2| {
            if !negated_clause_2.reconcilable || negated_clause_2.wedge {
                return None;
            }

            formula_1
                .iter()
                .find(|clause_1| contains_possibilities_of(negated_clause_2, clause_1))
        })
        .collect::<Vec<_>>();

    for (negated_clause_2, clause_1) in negated_formula_2.iter().zip(&contradicted_clauses) {
        let Some(clause_1) = clause_1 else {
            continue;
        };

        // The condition may hold in other ways, and those can use the same parts:
        // `($a is A || $a is B) && ($b is A || $b is B)` has nothing to remove after
        // `$a is B && $b is B` was ruled out. Only a part that no other way uses is a paradox.
        let has_part_of_no_other_way =
            negated_clause_2
                .possibilities
                .iter()
                .any(|(key, possibilities)| {
                    possibilities.keys().any(|assertion_hash| {
                        !negated_formula_2.iter().zip(&contradicted_clauses).any(
                            |(other_clause, other_clause_1)| {
                                other_clause_1.is_none()
                                    && other_clause
                                        .possibilities
                                        .get(key)
                                        .is_some_and(|p| p.contains_key(assertion_hash))
                            },
                        )
                    })
                });

        if !has_part_of_no_other_way {
            continue;
        }

        let mini_formula_2 = hakana_algebra::negate_formula(vec![negated_clause_2.clone()]);

        if let Ok(mini_formula_2) = mini_formula_2 {
            let mut paradox_message = String::new();
            if !mini_formula_2.first().unwrap().wedge {
                paradox_message += "Condition (";
                if mini_formula_2.len() > 1 {
                    paradox_message += "(";
                    paradox_message += mini_formula_2
                        .iter()
                        .map(|c| c.to_string(statements_analyzer.interner))
                        .collect::<Vec<String>>()
                        .join(") && (")
                        .as_str();
                    paradox_message += ")"
                } else {
                    paradox_message += mini_formula_2
                        .first()
                        .unwrap()
                        .to_string(statements_analyzer.interner)
                        .as_str();
                }
            } else {
                paradox_message += "Condition not (";
                paradox_message += negated_clause_2
                    .to_string(statements_analyzer.interner)
                    .as_str();
            }

            paradox_message += ") contradicts a previously-established condition (";
            paradox_message += clause_1.to_string(statements_analyzer.interner).as_str();
            paradox_message += ")";

            analysis_data.maybe_add_issue(
                Issue::new(
                    IssueKind::ParadoxicalCondition,
                    paradox_message,
                    statements_analyzer.get_hpos(pos),
                    calling_functionlike_id,
                ),
                statements_analyzer.get_config(),
                statements_analyzer.get_file_path_actual(),
            );

            return;
        }
    }
}

/// Whether everything `clause_1` allows is also allowed by `negated_clause_2`
fn contains_possibilities_of(negated_clause_2: &Clause, clause_1: &Clause) -> bool {
    if !clause_1.reconcilable || clause_1.wedge {
        return false;
    }

    for (key, clause_1_possibilities) in &clause_1.possibilities {
        if negated_clause_2.possibilities.get(key) != Some(clause_1_possibilities) {
            return false;
        }

        if clause_1_possibilities
            .values()
            .any(|p| matches!(p, Assertion::InArray(_) | Assertion::NotInArray(_)))
        {
            return false;
        }
    }

    true
}
