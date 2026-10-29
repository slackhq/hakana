enum class Columns: mixed {
    int count = 0;
    string name = '';
}

final class Expression<TEnum, +TValue> {}

function expression<TEnum, TValue>(
    HH\EnumClass\Label<TEnum, TValue> $label,
): Expression<TEnum, TValue> {
    invariant_violation('stub');
}

function accept_int(Expression<Columns, int> $expression): void {}

function missing_label(): void {
    $value = expression(#missing);
    accept_int($value);
}

function wrong_value(): void {
    $value = expression(#name);
    accept_int($value);
}
