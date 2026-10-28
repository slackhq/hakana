use function HH\legacy_is_truthy as truthy;

type Count = int;
newtype UserIds as vec<int> = vec<int>;
case type CountOrFlag = int | bool;

enum State: int {
    OFF = 0;
    ON = 1;
}

function integer(int $value): bool {
    return \HH\legacy_is_truthy($value);
}

function decimal(float $value): bool {
    return \HH\legacy_is_truthy($value);
}

function number(num $value): bool {
    return \HH\legacy_is_truthy($value);
}

function boolean(bool $value): bool {
    return \HH\legacy_is_truthy($value);
}

function vector(vec<string> $value): bool {
    return \HH\legacy_is_truthy($value);
}

function dictionary(dict<string, mixed> $value): bool {
    return \HH\legacy_is_truthy($value);
}

function setOfKeys(keyset<string> $value): bool {
    return \HH\legacy_is_truthy($value);
}

function tupleValue((int, string) $value): bool {
    return \HH\legacy_is_truthy($value);
}

function shapeValue(shape('name' => string) $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nullableInteger(?int $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nullableBoolean(?bool $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nullableVector(?vec<int> $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nullValue(null $value): bool {
    return \HH\legacy_is_truthy($value);
}

function numericEnum(State $value): bool {
    return \HH\legacy_is_truthy($value);
}

function typeAlias(Count $value): bool {
    return truthy($value);
}

function newtypeCollection(UserIds $value): bool {
    return \HH\legacy_is_truthy($value);
}

function mixedNonStringUnion(CountOrFlag $value): bool {
    return \HH\legacy_is_truthy($value);
}

function genericInteger<T as int>(T $value): bool {
    return \HH\legacy_is_truthy($value);
}

function genericCollection<T as Container<string>>(T $value): bool {
    return \HH\legacy_is_truthy($value);
}

function objectCollection(Vector<int> $value): bool {
    return \HH\legacy_is_truthy($value);
}

interface StringContainer extends Container<string> {}

function inheritedContainer(StringContainer $value): bool {
    return \HH\legacy_is_truthy($value);
}

final class Example {}

function objectValue(Example $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nullableObjectValue(?Example $value): bool {
    return \HH\legacy_is_truthy($value);
}

function narrowedMixed(mixed $value): bool {
    if ($value is int) {
        return \HH\legacy_is_truthy($value);
    }
    return false;
}

function collectionUnion(bool $choose_vec, vec<int> $items, dict<string, string> $map): bool {
    return \HH\legacy_is_truthy($choose_vec ? $items : $map);
}

abstract class Bounds {
    abstract const type TCount as int;

    public static function boundedTypeConstant(this::TCount $value): bool {
        return \HH\legacy_is_truthy($value);
    }
}

function ignoredInteger(int $value): bool {
    /* HAKANA_FIXME[UnnecessaryLegacyIsTruthy] This wrapper is deliberately retained. */
    return \HH\legacy_is_truthy($value);
}
