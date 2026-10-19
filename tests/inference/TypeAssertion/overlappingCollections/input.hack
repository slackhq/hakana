<<Hakana\TypeAssertion('$value', 'T')>>
function verify<reify T>(mixed $value): ?T {
    return $value ?as T;
}

function empty_vectors(vec<int> $value): void {
    verify<vec<string>>($value);
    $value ?as vec<string>;
}

function empty_dicts(dict<int, int> $value): void {
    verify<dict<string, string>>($value);
    $value ?as dict<string, string>;
}

function empty_keysets(keyset<int> $value): void {
    verify<keyset<string>>($value);
    $value ?as keyset<string>;
}

function traversables(vec<mixed> $value, dict<string, mixed> $dict, keyset<arraykey> $keys): void {
    verify<Traversable<string>>($value);
    $value ?as Traversable<string>;
    verify<KeyedTraversable<string, string>>($dict);
    $dict ?as KeyedTraversable<string, string>;
    verify<Traversable<int>>($keys);
    $keys ?as Traversable<int>;
}

function nonempty_overlap(arraykey $item): void {
    verify<vec<string>>(vec[$item]);
    vec[$item] ?as vec<string>;
    verify<Traversable<string>>(vec[$item]);
    vec[$item] ?as Traversable<string>;
}
