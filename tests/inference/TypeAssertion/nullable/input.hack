type int_type = int;
type nullable_int_type = ?int;

<<Hakana\TypeAssertion('$value', 'T')>>
function verify_type<T>(mixed $value, typename<T> $type): ?T {
    throw new Exception();
}

<<Hakana\TypeAssertion('$value', 'T')>>
function verify_reified<reify T>(mixed $value): ?T {
    return $value ?as T;
}

function impossible(string $value): void {
    verify_type($value, int_type::class);
    verify_reified<int>($value);
    verify_type(null, int_type::class);
    verify_reified<int>(null);
}

function redundant(int $value, ?int $nullable): void {
    verify_type($value, int_type::class);
    verify_reified<int>($value);
    verify_type($nullable, nullable_int_type::class);
    verify_reified<?int>($nullable);
    verify_type(null, nullable_int_type::class);
}

function uncertain(?int $nullable, mixed $unknown): void {
    verify_type($nullable, int_type::class);
    verify_reified<int>($nullable);
    verify_type($unknown, int_type::class);
    verify_reified<int>($unknown);
}

function input_is_not_narrowed(?int $value): ?int {
    verify_type($value, int_type::class);
    return $value;
}
