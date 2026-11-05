enum StringStatus: string {
    OPEN = 'open';
    CLOSED = 'closed';
}

enum IntStatus: int {
    OPEN = 1;
    CLOSED = 2;
}

abstract class Base {}
final class Child extends Base {}

function checkArraykey(dict<string, int> $items, arraykey $lookup): void {
    C\contains_key($items, $lookup);
    Dict\contains_key($items, $lookup);
    C\contains($items, $lookup);
    Dict\contains($items, $lookup);
}

function checkNullable(vec<string> $items, ?string $lookup): bool {
    return C\contains($items, $lookup);
}

function checkEnumValues(
    vec<string> $strings,
    vec<int> $ints,
    StringStatus $string_status,
    IntStatus $int_status,
): void {
    C\contains($strings, $string_status);
    C\contains($ints, $int_status);
    C\contains($ints, IntStatus::OPEN);
    C\contains(vec[StringStatus::OPEN], 'open');
}

function checkStringAgainstEnum(vec<StringStatus> $items, string $lookup): bool {
    return C\contains($items, $lookup);
}

function checkClassname(classname<Base> $lookup, string $name): void {
    C\contains(vec[nameof Child], $lookup);
    C\contains_key(dict[nameof Child => true], $lookup);
    C\contains_key(dict[nameof Child => true], $name);
}

function checkClassPtr(class<Base> $lookup): bool {
    return C\contains(vec[Child::class], $lookup);
}
