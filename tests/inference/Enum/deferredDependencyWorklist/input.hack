enum class Columns: mixed {
    int count = 0;
    string name = '';
}

final class Expression<TEnum, +TValue> {}

function expression<TEnum, TValue>(
    HH\EnumClass\Label<TEnum, TValue> $label,
)[]: Expression<TEnum, TValue> {
    invariant_violation('stub');
}

function relay<TEnum, TValue>(
    Expression<TEnum, TValue> $value,
)[]: Expression<TEnum, TValue> {
    return $value;
}

function value<TEnum, TValue>(Expression<TEnum, TValue> $expression)[]: TValue {
    invariant_violation('stub');
}

function accept_int(Expression<Columns, int> $expression)[]: void {}
function accept_string(Expression<Columns, string> $expression)[]: void {}

function indirect_dependencies(): int {
    $first = expression(#count);
    $second = relay($first);
    $third = relay($second);
    $independent = expression(#name);
    accept_string($independent);
    accept_int($third);
    return value($first);
}

function closure_inherits_dependencies(): int {
    $first = expression(#count);
    $alias = relay($first);
    $infer = () ==> {
        accept_int($alias);
    };
    $infer();
    return value($first);
}

function closure_merges_new_dependencies(): int {
    $first = expression(#count);
    $infer = () ==> {
        $independent = expression(#name);
        accept_string($independent);
        accept_int(relay($first));
    };
    $infer();
    return value($first);
}

function missing_label_through_alias(): void {
    $first = expression(#missing);
    $alias = relay($first);
    accept_int($alias);
}

function wrong_value_through_alias(): void {
    $first = expression(#name);
    $alias = relay($first);
    accept_int($alias);
}
