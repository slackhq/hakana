function take_ints(Container<int> $values): void {}

function flattened(dict<int, vec<keyset<int>>> $values): void {
    $flattened = \HH\Lib\Dict\map($values, \HH\Lib\Keyset\flatten<>);
    foreach ($flattened as $value) {
        take_ints($value);
    }
}

function wrong_elements(dict<int, vec<keyset<string>>> $values): void {
    $flattened = \HH\Lib\Dict\map($values, \HH\Lib\Keyset\flatten<>);
    foreach ($flattened as $value) {
        take_ints($value);
    }
}

function string_identity<T as string>(T $value): T {
    return $value;
}

function invalid_bound(vec<int> $values): vec<int> {
    return \HH\Lib\Vec\map($values, string_identity<>);
}
