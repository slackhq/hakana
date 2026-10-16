type int_type = int;
type data_type = shape('id' => int);

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_type<T>(mixed $value, typename<T> $type): T {
    throw new Exception();
}

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_explicit<T>(mixed $value): T {
    throw new Exception();
}

function unannotated<T>(mixed $value, typename<T> $type): T {
    throw new Exception();
}

function uncertain(mixed $unknown, arraykey $key, ?int $nullable, dict<string, mixed> $data): void {
    assert_type($unknown, int_type::class);
    assert_type($key, int_type::class);
    assert_type($nullable, int_type::class);
    assert_type($data, data_type::class);
    assert_explicit($unknown);
    unannotated('hello', int_type::class);
    unannotated(123, int_type::class);
}

function generic<T>(mixed $value, typename<T> $type): T {
    return assert_type($value, $type);
}

function bounded_generic<T as int>(int $value, typename<T> $type): T {
    return assert_type($value, $type);
}

function no_input_narrowing(arraykey $value): string {
    assert_type($value, int_type::class);
    return $value as string;
}

function unresolved_assertion(int $value): void {
    assert_explicit($value);
}

function nested_generic<T as int>(vec<int> $value): void {
    assert_explicit<vec<T>>($value);
}
