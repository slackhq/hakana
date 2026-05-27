function some_cond(int $i): bool {
    return $i % 2 === 1;
}

final class C {}

function maybe_object(bool $test): ?C {
    return $test ? new C() : null;
}

function main(bool $input, int $x): int {
    if ($x && $input && maybe_object($input) && some_cond($x)) {
        echo "test";
    }

    if ($input || !maybe_object($input) || !some_cond($x)) {
        echo "test";
    }

    if (maybe_object($input)) {
        echo "test";
    }

    if (!maybe_object($input)) {
        echo "test";
    }

    if ($input && !(maybe_object($input) || some_cond($x))) {
        echo "test";
    }

    return maybe_object($input) ? 5 : 4;
}
