final class C {}

enum StringEnum: string {
    VALUE = 'value';
}

function get_value(): ?string {
    return null;
}

function get_enum(): ?StringEnum {
    return null;
}

function fallback(): string {
    return "";
}

function get_string(): string {
    return "";
}

function conditions(
    ?string $value,
    ?string $other,
    string $string,
    bool $bool,
    ?C $object,
    int $integer,
    shape(?'value' => string, ?'other' => string) $input
): string {
    $enum = get_enum();
    if ($enum) {
        echo "enum";
    }

    $explicit_bool = !!get_string();

    for (; $string; $integer++) {
        break;
    }

    if ($value) {
        echo "direct";
    }

    if (!$value) {
        echo "negated";
    }

    if (get_value()) {
        echo "non-simple";
    }

    if (!get_value()) {
        echo "negated non-simple";
    }

    if (!!$value) {
        echo "double negated";
    }

    if ($bool && $value || !$other) {
        echo "logical";
    }

    if ($string) {
        echo "string";
    }

    if ($object) {
        echo "object";
    }

    if ($integer) {
        echo "integer";
    }

    if ($input['value'] ?? false) {
        echo "foo";
    }

    if ($input['value'] ?? fallback()) {
        echo "foo";
    }

    if ($input['other'] ?? $input['value'] ?? false) {
        echo "foo";
    }

    return $value ? "present" : "absent";
}
