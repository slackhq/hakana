function checkLiteralVec(string $some_string): bool {
    return HH\Lib\C\contains_key(vec['a', 'b', 'c'], $some_string);
}

function checkLiteralVecWithLiteralKey(): bool {
    return HH\Lib\C\contains_key(vec['a', 'b', 'c'], 'a');
}

function getValues(): vec<string> {
    return vec['a', 'b', 'c'];
}

function checkReturnedVec(string $key): bool {
    return HH\Lib\C\contains_key(getValues(), $key);
}

function checkVecVariable(vec<string> $values, string $key): bool {
    return HH\Lib\C\contains_key($values, $key);
}

function checkIntegerKey(int $key): bool {
    return HH\Lib\C\contains_key(vec['a', 'b', 'c'], $key);
}

function checkLiteralIntegerKey(): bool {
    return HH\Lib\C\contains_key(vec['a', 'b', 'c'], 1);
}

function checkArraykey(arraykey $key): bool {
    return HH\Lib\C\contains_key(vec['a', 'b', 'c'], $key);
}

function checkDictKey(string $key): bool {
    return HH\Lib\C\contains_key(dict['a' => 1, 'b' => 2], $key);
}

function checkKeysetKey(string $key): bool {
    return HH\Lib\C\contains_key(keyset['a', 'b', 'c'], $key);
}

function checkValue(string $value): bool {
    return HH\Lib\C\contains(vec['a', 'b', 'c'], $value);
}

function checkGenericVec<T>(vec<T> $values, string $key): bool {
    return HH\Lib\C\contains_key($values, $key);
}

function checkExplicitKeyType(string $key): bool {
    return HH\Lib\C\contains_key<int, arraykey, string>(vec['a', 'b', 'c'], $key);
}

function checkUnion(bool $use_vec, string $key): bool {
    $values = $use_vec ? vec['a', 'b', 'c'] : dict['a' => 'b'];
    return HH\Lib\C\contains_key($values, $key);
}

function checkEmptyVec(int $key): bool {
    return HH\Lib\C\contains_key(vec[], $key);
}
