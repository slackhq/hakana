enum class Columns: mixed {
    int count = 0;
    string name = '';
}

final class EnumType<T> {}
final class Expression<TEnum, +TValue> {}

function expression<TEnum, TValue>(
    HH\EnumClass\Label<TEnum, TValue> $label,
): Expression<TEnum, TValue> {
    invariant_violation('stub');
}

function with_enum<TEnum, TValue>(
    Expression<TEnum, TValue> $expression,
    EnumType<TEnum> $enum,
): TValue {
    invariant_violation('stub');
}

function missing_label(EnumType<Columns> $enum): int {
    return with_enum(expression(#missing), $enum);
}

function wrong_value(EnumType<Columns> $enum): string {
    return with_enum(expression(#count), $enum);
}

function no_enum_context(): void {
    $value = expression(#count);
    $_ = $value;
}
