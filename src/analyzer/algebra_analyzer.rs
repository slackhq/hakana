use std::{collections::HashSet, rc::Rc};

use hakana_algebra::Clause;

use hakana_reflection_info::{
    assertion::Assertion,
    issue::{Issue, IssueKind},
};
use oxidized::ast::Pos;

use crate::{statements_analyzer::StatementsAnalyzer, typed_ast::TastInfo};

pub(crate) fn check_for_paradox(
    statements_analyzer: &StatementsAnalyzer,
    formula_1: &Vec<Rc<Clause>>,
    formula_2: &Vec<Clause>,
    tast_info: &mut TastInfo,
    pos: &Pos,
) {
    let negated_formula_2 = hakana_algebra::negate_formula(formula_2.clone());

    if let Err(_) = negated_formula_2 {
        return;
    }

    let negated_formula_2 = negated_formula_2.unwrap();

    let formula_1_hashes: HashSet<&Clause> = HashSet::from_iter(formula_1.iter().map(|v| &**v));

    let mut formula_2_hashes = HashSet::new();

    for formula_2_clause in formula_2 {
        if !formula_2_clause.generated
            && !formula_2_clause.wedge
            && formula_2_clause.reconcilable
            && (formula_1_hashes.contains(formula_2_clause)
                || formula_2_hashes.contains(formula_2_clause))
        {
            tast_info.maybe_add_issue(Issue::new(
                IssueKind::RedundantTypeComparison,
                format!(
                    "{} {}",
                    formula_2_clause.to_string(),
                    "has already been asserted"
                ),
                statements_analyzer.get_hpos(&pos),
            ));
        }

        formula_2_hashes.insert(formula_2_clause);
    }

    for negated_clause_2 in &negated_formula_2 {
        if !negated_clause_2.reconcilable || negated_clause_2.wedge {
            continue;
        }

        for clause_1 in formula_1 {
            if !clause_1.reconcilable || clause_1.wedge {
                continue;
            }

            let mut negated_clause_2_contains_1_possibilities = true;

            'outer: for (key, clause_1_possibilities) in &clause_1.possibilities {
                if let Some(clause_2_possibilities) = negated_clause_2.possibilities.get(key) {
                    if clause_2_possibilities != clause_1_possibilities {
                        negated_clause_2_contains_1_possibilities = false;
                        break;
                    }
                } else {
                    negated_clause_2_contains_1_possibilities = false;
                    break;
                }

                for (_, possibility) in clause_1_possibilities {
                    if let Assertion::InArray(_) | Assertion::NotInArray(_) = possibility {
                        negated_clause_2_contains_1_possibilities = false;
                        break 'outer;
                    }
                }
            }

            if negated_clause_2_contains_1_possibilities {
                let mini_formula_2 = hakana_algebra::negate_formula(vec![negated_clause_2.clone()]);

                if let Ok(mini_formula_2) = mini_formula_2 {
                    let mut paradox_message = String::new();
                    if !mini_formula_2.get(0).unwrap().wedge {
                        paradox_message += "Condition (";
                        if mini_formula_2.len() > 1 {
                            paradox_message += "(";
                            paradox_message += mini_formula_2
                                .iter()
                                .map(|c| c.to_string())
                                .collect::<Vec<String>>()
                                .join(") && (")
                                .as_str();
                            paradox_message += ")"
                        } else {
                            paradox_message += mini_formula_2.get(0).unwrap().to_string().as_str();
                        }
                    } else {
                        paradox_message += "Condition not (";
                        paradox_message += negated_clause_2.to_string().as_str();
                    }

                    paradox_message += ") contradicts a previously-established condition (";
                    paradox_message += clause_1.to_string().as_str();
                    paradox_message += ")";

                    tast_info.maybe_add_issue(Issue::new(
                        IssueKind::ParadoxicalCondition,
                        paradox_message,
                        statements_analyzer.get_hpos(&pos),
                    ));

                    return;
                }
            }
        }
    }
}
