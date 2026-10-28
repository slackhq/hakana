type Text = string;
newtype EncodedId as string = string;
case type StringOrItems = string | vec<int>;
case type ArraykeyOrItems = arraykey | vec<int>;

enum StringState: string {
    OFF = '0';
    ON = '1';
}

function stringValue(string $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nullableString(?string $value): bool {
    return \HH\legacy_is_truthy($value);
}

function stringLiteral(): bool {
    return \HH\legacy_is_truthy('0');
}

function truthyStringLiteral(): bool {
    return \HH\legacy_is_truthy('hello');
}

function arraykeyValue(arraykey $value): bool {
    return \HH\legacy_is_truthy($value);
}

function mixedValue(mixed $value): bool {
    return \HH\legacy_is_truthy($value);
}

function nonnullValue(nonnull $value): bool {
    return \HH\legacy_is_truthy($value);
}

function stringAlias(Text $value): bool {
    return \HH\legacy_is_truthy($value);
}

function stringNewtype(EncodedId $value): bool {
    return \HH\legacy_is_truthy($value);
}

function stringEnum(StringState $value): bool {
    return \HH\legacy_is_truthy($value);
}

function stringEnumCase(): bool {
    return \HH\legacy_is_truthy(StringState::OFF);
}

function stringOrCollection(StringOrItems $value): bool {
    return \HH\legacy_is_truthy($value);
}

function arraykeyOrCollection(ArraykeyOrItems $value): bool {
    return \HH\legacy_is_truthy($value);
}

function integerOrString(bool $choose_int, int $number, string $text): bool {
    return \HH\legacy_is_truthy($choose_int ? $number : $text);
}

function genericUnknown<T>(T $value): bool {
    return \HH\legacy_is_truthy($value);
}

function genericArraykey<T as arraykey>(T $value): bool {
    return \HH\legacy_is_truthy($value);
}

function genericString<T as string>(T $value): bool {
    return \HH\legacy_is_truthy($value);
}

function stringishValue(Stringish $value): bool {
    return \HH\legacy_is_truthy($value);
}

function xhpChildValue(XHPChild $value): bool {
    return \HH\legacy_is_truthy($value);
}

function className(classname<Exception> $value): bool {
    return \HH\legacy_is_truthy($value);
}

function classNameLiteral(): bool {
    return \HH\legacy_is_truthy(nameof Exception);
}

abstract class Bounds {
    abstract const type TStringOrInt as arraykey;

    public static function boundedTypeConstant(this::TStringOrInt $value): bool {
        return \HH\legacy_is_truthy($value);
    }
}
