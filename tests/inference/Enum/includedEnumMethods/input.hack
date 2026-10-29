enum Original: string as string {
    FIRST = 'first';
}

enum Included: string as string {
    use Original;
    SECOND = 'second';
}

enum Other: string as string {
    THIRD = 'third';
}

enum Nested: string as string {
    use Included;
    use Other;
    FOURTH = 'fourth';
}

function included_values(): dict<string, Included> {
    return Included::getValues();
}

function nested_values(): vec<Nested> {
    return vec(Nested::getValues());
}

function nested_names(): dict<Nested, string> {
    return Nested::getNames();
}

function nested_assert(string $value): Nested {
    return Nested::assert($value);
}

function nested_coerce(string $value): ?Nested {
    return Nested::coerce($value);
}

function nested_assert_all(vec<string> $values): Container<Nested> {
    return Nested::assertAll($values);
}

function mapped_assert(vec<string> $values): keyset<Nested> {
    return \HH\Lib\Keyset\map($values, $value ==> Nested::assert($value));
}

function original_values(): dict<string, Original> {
    return Original::getValues();
}

function included_constant(): Nested {
    return Nested::FIRST;
}
