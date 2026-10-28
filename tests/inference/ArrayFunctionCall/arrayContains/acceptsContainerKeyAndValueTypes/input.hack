interface StringKeyedContainer extends KeyedContainer<string, int> {}
interface IntTraversable extends Traversable<int> {}

function checkDictKey(dict<string, int> $items, string $key): bool {
    return C\contains_key($items, $key);
}

function checkLiteralDictKey(string $key): bool {
    return C\contains_key(dict['a' => 1, 'b' => 2], $key);
}

function checkKeysetKey(keyset<string> $items, string $key): bool {
    return C\contains_key($items, $key);
}

function checkInheritedKey(StringKeyedContainer $items, string $key): bool {
    return C\contains_key($items, $key);
}

function checkVectorKey(Vector<string> $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkGenericKey<Tk as arraykey, Tv>(KeyedContainer<Tk, Tv> $items, Tk $key): bool {
    return C\contains_key($items, $key);
}

function checkLiteralValue(string $value): bool {
    return C\contains(vec['a', 'b'], $value);
}

function checkLargeLiteralCollection(string $key, string $value): bool {
    $items = keyset[
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h',
        'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p',
        'q', 'r', 's', 't', 'u', 'v', 'w', 'x',
    ];
    return C\contains_key($items, $key) && C\contains($items, $value);
}

function checkNonEmptyString(string $member, string $lookup): bool {
    if ($member === '') {
        return false;
    }
    return C\contains_key(dict[$member => true], $lookup) &&
        C\contains(vec[$member], $lookup);
}

function checkDictValue(dict<string, int> $items, int $value): bool {
    return C\contains($items, $value);
}

function checkInheritedValue(IntTraversable $items, int $value): bool {
    return C\contains($items, $value);
}

function checkGenericValue<T>(Traversable<T> $items, T $value): bool {
    return C\contains($items, $value);
}

function checkNullableValue(?string $value): bool {
    return C\contains(vec['a', null], $value);
}

function checkMixedValue(vec<mixed> $items, mixed $value): bool {
    return C\contains($items, $value);
}

function checkUnionKey(bool $use_vec, arraykey $key): bool {
    $items = $use_vec ? vec[1, 2] : dict['a' => 3];
    return C\contains_key($items, $key);
}

function checkUnionValue(bool $use_vec, arraykey $value): bool {
    $items = $use_vec ? vec['a'] : dict['b' => 1];
    return C\contains($items, $value);
}

enum Status: string {
    OPEN = 'open';
    CLOSED = 'closed';
}

function checkLiteralEnumKey(Status $key): bool {
    return C\contains_key(dict[Status::OPEN => 1], $key);
}

function checkLiteralEnumValue(Status $value): bool {
    return C\contains(vec[Status::OPEN], $value);
}

function checkLiteralBoolValue(bool $value): bool {
    return C\contains(vec[true], $value);
}

abstract class Collections {
    abstract const type TItems as KeyedContainer<string, int>;

    public static function checkKey(this::TItems $items, string $key): bool {
        return C\contains_key($items, $key);
    }

    public static function checkValue(this::TItems $items, int $value): bool {
        return C\contains($items, $value);
    }
}

final class Example {}

function checkLiteralClassname(classname<Example> $key): bool {
    return C\contains_key(dict[nameof Example => 1], $key);
}

function checkLiteralClassPtr(class<Example> $value): bool {
    return C\contains(vec[Example::class], $value);
}
