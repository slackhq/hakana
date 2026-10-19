<<file: __EnableUnstableFeatures('named_parameters', 'named_parameters_use')>>

type int_type = int;

<<Hakana\TypeAssertion('value', 'T')>>
function check<T>(string $label, named typename<T> $expected, named mixed $value): ?T {
    throw new Exception();
}

<<Hakana\TypeAssertion('$value', 'T')>>
function second<T>(typename<T> $type, mixed $value): T {
    throw new Exception();
}

function test(string $bad, int $good, mixed $unknown): void {
    check('bad', value=$bad, expected=int_type::class);
    check('good', expected=int_type::class, value=$good);
    check('unknown', value=$unknown, expected=int_type::class);
    second(int_type::class, $bad);
    second(int_type::class, $good);
}
