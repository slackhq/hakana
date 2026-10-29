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

function enum_value<TEnum, TValue>(
    HH\EnumClass\Label<TEnum, TValue> $label,
    EnumType<TEnum> $enum,
): TValue {
    invariant_violation('stub');
}

function with_enum<TEnum, TValue>(
    Expression<TEnum, TValue> $expression,
    EnumType<TEnum> $enum,
): TValue {
    invariant_violation('stub');
}

function sibling_argument(EnumType<Columns> $enum): int {
    return enum_value(#count, $enum);
}

function nested_sibling_argument(EnumType<Columns> $enum): int {
    // The inner call cannot know its enum until the outer call checks $enum.
    return with_enum(expression(#count), $enum);
}

function nested_string_argument(EnumType<Columns> $enum): string {
    return with_enum(expression(#name), $enum);
}
