type StringDict = dict<string, int>;
newtype IntDict as dict<int, string> = dict<int, string>;

interface StringKeyedContainer extends KeyedContainer<string, int> {}
interface IntTraversable extends Traversable<int> {}

function checkDictKey(dict<string, int> $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkLiteralDictKey(string $key): bool {
    return C\contains_key(dict[1 => 'a', 2 => 'b'], $key);
}

function checkKeysetKey(keyset<string> $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkInterfaceKey(KeyedContainer<string, int> $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkInheritedKey(StringKeyedContainer $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkVectorKey(Vector<string> $items, string $key): bool {
    return C\contains_key($items, $key);
}

function checkMapKey(Map<string, int> $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkGenericKey<T as KeyedContainer<string, int>>(T $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkAliasKey(StringDict $items, int $key): bool {
    return C\contains_key($items, $key);
}

function checkNewtypeKey(IntDict $items, string $key): bool {
    return C\contains_key($items, $key);
}

function checkVecValue(vec<int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkDictValue(dict<string, int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkKeysetValue(keyset<string> $items, int $value): bool {
    return C\contains($items, $value);
}

function checkTraversableValue(Traversable<int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkContainerValue(Container<int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkKeyedTraversableValue(KeyedTraversable<string, int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkInheritedValue(IntTraversable $items, string $value): bool {
    return C\contains($items, $value);
}

function checkVectorValue(Vector<int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkMapValue(Map<string, int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkSetValue(Set<int> $items, string $value): bool {
    return C\contains($items, $value);
}

function checkGenericValue<T as Traversable<int>>(T $items, string $value): bool {
    return C\contains($items, $value);
}

function checkAliasValue(StringDict $items, string $value): bool {
    return C\contains($items, $value);
}

function checkExplicitValueType(string $value): bool {
    return C\contains<mixed, string>(vec[1, 2, 3], $value);
}

abstract class Collections {
    abstract const type TItems as KeyedContainer<string, int>;

    public static function checkKey(this::TItems $items, int $key): bool {
        return C\contains_key($items, $key);
    }

    public static function checkValue(this::TItems $items, string $value): bool {
        return C\contains($items, $value);
    }
}
