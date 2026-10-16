type int_type = int;
type string_type = string;

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_type<T>(mixed $value, typename<T> $type): T {
    throw new Exception();
}

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_structure<T>(mixed $value, TypeStructure<T> $type): T {
    throw new Exception();
}

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_unsafe<T>(mixed $value, typename<T> $type, float $sample_rate = 1.0): T {
    throw new Exception();
}

<<Hakana\TypeAssertion('$value', 'T')>>
function assert_reified<reify T>(mixed $value): T {
    return $value as T;
}

<<Hakana\TypeAssertion('$value', 'T')>>
function check_error<T>(mixed $value, typename<T> $type): ?Exception {
    return null;
}

function impossible(string $value, TypeStructure<int> $structure): void {
    assert_type($value, int_type::class);
    assert_structure($value, $structure);
    assert_unsafe($value, int_type::class, 0.0);
    assert_reified<int>($value);
    check_error($value, int_type::class);
}

function redundant(int $value, TypeStructure<int> $structure): void {
    assert_type($value, int_type::class);
    assert_structure($value, $structure);
    assert_unsafe($value, int_type::class);
    assert_reified<int>($value);
    check_error($value, int_type::class);
}

function expressions(): void {
    assert_type(123, string_type::class);
    assert_type('hello', string_type::class);
    assert_reified<int>(HH\Lib\Str\length('hello'));
}

function generic_impossible<T as int>(string $value, typename<T> $type): void {
    assert_type($value, $type);
}

function generic_redundant<T>(T $value, typename<T> $type): void {
    assert_type($value, $type);
}
