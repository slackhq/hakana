enum Number: int {
    ONE = 1;
}

<<Hakana\TypeAssertion('$value', 'T')>>
function verify<reify T>(mixed $value): ?T {
    return $value ?as T;
}

function numeric_strings(string $value): void {
    verify<Number>('1');
    '1' ?as Number;
    verify<Number>($value);
    $value ?as Number;
    verify<dict<Number, string>>(dict['1' => 'value']);
    dict['1' => 'value'] ?as dict<Number, string>;
}

function nonnumeric_strings(): void {
    verify<Number>('other');
    'other' ?as Number;
}
