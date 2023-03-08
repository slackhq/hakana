use std::collections::HashMap;

use hakana_reflection_info::{
    ast_signature::DefSignatureNode, diff::CodebaseDiff, file_info::FileInfo, StrId, STR_EMPTY,
};
use rustc_hash::FxHashMap;

pub(crate) fn get_diff(
    existing_files: &FxHashMap<StrId, FileInfo>,
    new_files: &FxHashMap<StrId, FileInfo>,
) -> CodebaseDiff {
    let mut keep = vec![];
    let mut keep_signature = vec![];
    let mut add_or_delete = vec![];
    let mut diff_map = FxHashMap::default();
    let mut deletion_ranges_map = FxHashMap::default();

    for (file_id, new_file_info) in new_files {
        if let Some(existing_file_info) = existing_files.get(file_id) {
            let mut file_diffs = vec![];
            let mut deletion_ranges = vec![];

            let (trace, x, y, bc) =
                calculate_trace(&existing_file_info.ast_nodes, &new_file_info.ast_nodes);

            let diff = extract_diff(
                trace,
                x,
                y,
                &existing_file_info.ast_nodes,
                &new_file_info.ast_nodes,
                bc,
            );

            for diff_elem in diff {
                match diff_elem {
                    AstDiffElem::Keep(a, b) => {
                        let mut has_change = false;

                        let (class_trace, class_x, class_y, class_bc) =
                            calculate_trace(&a.children, &b.children);

                        let class_diff = extract_diff(
                            class_trace,
                            class_x,
                            class_y,
                            &a.children,
                            &b.children,
                            class_bc,
                        );

                        for class_diff_elem in class_diff {
                            match class_diff_elem {
                                AstDiffElem::Keep(a_child, b_child) => {
                                    keep.push((a.name, a_child.name));

                                    if b_child.start_offset != a_child.start_offset
                                        || b_child.start_line != a_child.start_line
                                    {
                                        file_diffs.push((
                                            a_child.start_offset,
                                            a_child.end_offset,
                                            b_child.start_offset as isize
                                                - a_child.start_offset as isize,
                                            b_child.start_line as isize
                                                - a_child.start_line as isize,
                                        ));
                                    }
                                }
                                AstDiffElem::KeepSignature(a_child, _) => {
                                    has_change = true;
                                    keep_signature.push((a.name, a_child.name));
                                    deletion_ranges
                                        .push((a_child.start_offset, a_child.end_offset));
                                }
                                AstDiffElem::Remove(child_node) => {
                                    has_change = true;
                                    add_or_delete.push((a.name, child_node.name));
                                    deletion_ranges
                                        .push((child_node.start_offset, child_node.end_offset));
                                }
                                AstDiffElem::Add(child_node) => {
                                    has_change = true;
                                    add_or_delete.push((a.name, child_node.name));
                                }
                            }
                        }

                        if has_change {
                            keep_signature.push((a.name, STR_EMPTY));
                        } else {
                            keep.push((a.name, STR_EMPTY));

                            if b.start_offset != a.start_offset || b.start_line != a.start_line {
                                file_diffs.push((
                                    a.start_offset,
                                    a.end_offset,
                                    b.start_offset as isize - a.start_offset as isize,
                                    b.start_line as isize - a.start_line as isize,
                                ));
                            }
                        }
                    }
                    AstDiffElem::KeepSignature(a, _) => {
                        keep_signature.push((a.name, STR_EMPTY));
                        deletion_ranges.push((a.start_offset, a.end_offset));
                    }
                    AstDiffElem::Remove(node) => {
                        add_or_delete.push((node.name, STR_EMPTY));
                        deletion_ranges.push((node.start_offset, node.end_offset));
                    }
                    AstDiffElem::Add(node) => {
                        add_or_delete.push((node.name, STR_EMPTY));
                    }
                }
            }

            if !file_diffs.is_empty() {
                diff_map.insert(*file_id, file_diffs);
            }

            if !deletion_ranges.is_empty() {
                deletion_ranges_map.insert(*file_id, deletion_ranges);
            }
        }
    }

    CodebaseDiff {
        keep,
        keep_signature,
        add_or_delete,
        diff_map,
        deletion_ranges_map,
    }
}

/**
 * Borrows from https://github.com/nikic/PHP-Parser/blob/master/lib/PhpParser/Internal/Differ.php
 *
 * Implements the Myers diff algorithm.
 *
 * Myers, Eugene W. "An O (ND) difference algorithm and its variations."
 * Algorithmica 1.1 (1986): 251-266.
 */
pub(crate) fn calculate_trace(
    a_nodes: &Vec<DefSignatureNode>,
    b_nodes: &Vec<DefSignatureNode>,
) -> (
    Vec<FxHashMap<isize, usize>>,
    usize,
    usize,
    FxHashMap<usize, bool>,
) {
    let n = a_nodes.len();
    let m = b_nodes.len();
    let max = n + m;
    let mut v: HashMap<isize, usize, _> = FxHashMap::default();
    v.insert(1, 0);
    let mut bc = FxHashMap::default();
    let mut trace = vec![];
    for d in 0..=(max as isize) {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let mut x = if k == -d || (k != d && v[&(k - 1)] < v[&(k + 1)]) {
                v[&(k + 1)]
            } else {
                v[&(k - 1)] + 1
            };

            let mut y = (x as isize - k) as usize;

            let mut body_change = false;

            while x < n
                && y < m
                && is_equal(
                    a_nodes.get(x).unwrap(),
                    b_nodes.get(y).unwrap(),
                    &mut body_change,
                )
            {
                bc.insert(x, body_change);
                x += 1;
                y += 1;
                body_change = false;
            }

            v.insert(k, x);

            if x >= n && y >= m {
                return (trace, x, y, bc);
            }
            k += 2;
        }
    }

    panic!();
}

fn is_equal(a_node: &DefSignatureNode, b_node: &DefSignatureNode, body_change: &mut bool) -> bool {
    if a_node.name != b_node.name
        || a_node.signature_hash != b_node.signature_hash
        || a_node.is_function != b_node.is_function
    {
        return false;
    }

    if a_node.body_hash != b_node.body_hash {
        *body_change = true;
    }

    return true;
}

pub(crate) fn extract_diff<'a>(
    trace: Vec<FxHashMap<isize, usize>>,
    mut x: usize,
    mut y: usize,
    a_nodes: &'a Vec<DefSignatureNode>,
    b_nodes: &'a Vec<DefSignatureNode>,
    bc: FxHashMap<usize, bool>,
) -> Vec<AstDiffElem<'a>> {
    let mut result = vec![];
    let mut d = trace.len() as isize - 1;

    while d >= 0 {
        let v = &trace[d as usize];
        let k = (x as isize) - (y as isize);

        let prev_k = if k == -d || (k != d && v[&(k - 1)] < v[&(k + 1)]) {
            k + 1
        } else {
            k - 1
        };

        let prev_x = v[&prev_k];
        let prev_y = prev_x as isize - prev_k;

        while x > prev_x && y as isize > prev_y {
            result.push(if bc[&(x - 1)] {
                AstDiffElem::KeepSignature(&a_nodes[x - 1], &b_nodes[y - 1])
            } else {
                AstDiffElem::Keep(&a_nodes[x - 1], &b_nodes[y - 1])
            });
            x -= 1;
            y -= 1;
        }

        if d == 0 {
            break;
        }

        while x > prev_x {
            result.push(AstDiffElem::Remove(&a_nodes[x - 1]));
            x -= 1;
        }

        while y as isize > prev_y {
            result.push(AstDiffElem::Add(&b_nodes[y - 1]));
            y -= 1;
        }

        d -= 1;
    }
    result.reverse();
    result
}

#[derive(Debug)]
pub(crate) enum AstDiffElem<'a> {
    Keep(&'a DefSignatureNode, &'a DefSignatureNode),
    KeepSignature(&'a DefSignatureNode, &'a DefSignatureNode),
    Remove(&'a DefSignatureNode),
    Add(&'a DefSignatureNode),
}
