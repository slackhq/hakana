final class C {}

function get_value(): ?string {
    return null;
}

function conditions(
    ?string $value,
    ?string $other,
    string $string,
    bool $bool,
    ?C $object,
    int $integer,
): string {
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

    return $value ? "present" : "absent";
}
