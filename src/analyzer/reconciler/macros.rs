#[macro_export]
macro_rules! intersect_simple {
    (
        $(|)? $( $subtype_pattern:pat_param )|+ $( if $subtype_guard: expr )? $(,)?,
        $(|)? $( $supertype_pattern:pat_param )|+ $( if $supertype_guard: expr )? $(,)?,
        $max_type:expr,
        $assertion:expr,
        $existing_var_type:expr,
        $key:expr,
        $negated:expr,
        $tast_info:expr,
        $statements_analyzer:expr,
        $pos:expr,
        $calling_functionlike_id:expr,
        $is_equality:expr,
        $suppressed_issues:expr,
    ) => {
        {
            let mut acceptable_types = Vec::new();
            let mut did_remove_type = false;

            for atomic in &$existing_var_type.types {
                if matches!(atomic, $( $subtype_pattern )|+ $( if $subtype_guard )?) {
                    acceptable_types.push(atomic.clone());
                } else if matches!(atomic, $( $supertype_pattern )|+ $( if $supertype_guard )?) {
                    return Some($max_type);
                } else {
                    did_remove_type = true;
                }
            }

            if acceptable_types.is_empty() || (!did_remove_type && !$is_equality) {
                if let Some(k) = $key {
                    if let Some(loc) = $pos {
                        trigger_issue_for_impossible(
                            $tast_info,
                            $statements_analyzer,
                            &$existing_var_type.get_id(Some(&$statements_analyzer.get_codebase().interner)),
                            &k,
                            $assertion,
                            !did_remove_type,
                            $negated,
                            loc,
                            $calling_functionlike_id,
                            $suppressed_issues,
                        );
                    }
                }
            }

            if !acceptable_types.is_empty() {
                return Some(TUnion::new(acceptable_types));
            }

            Some(get_nothing())
        }
    }
}
