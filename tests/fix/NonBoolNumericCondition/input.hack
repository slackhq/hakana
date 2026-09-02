function some_cond(int $i): bool {
    return $i++ % 2 === 1;
}

final class C {}

function maybe_object(bool $test): ?C {
    return $test ? new C() : null;
}

function maybe_int(bool $test): ?int {
    return $test ? 1 : null;
}

function main(bool $input, int $x, ?int $nullable_x, ?int $nullable_y, string $foo): int {
    if ($x && $input && maybe_object($input) && some_cond($x)) {
        echo "test";
    }

    if ($input || !maybe_object($input) || !some_cond($x)) {
        echo "test";
    }

    if ($input && !(maybe_object($input) || some_cond($x))) {
        echo "test";
    }

    if (!$x) {
        echo "test";
    }

    $bar = 4;
    if (rand(0,1) > 0) {
        $bar = 'baz';
    }

    if ($bar) {
        echo "test";
    }

    if (preg_match('/\s/', $foo)) {
        echo "test";
    }

    if (!!preg_match('/\s/', $foo)) {
        echo "test";
    }

    if ($input || !preg_match('/\s/', $foo)) {
        echo "test";
    }

    if ($nullable_x) {
        echo "test";
    }

    if (!$nullable_x) {
        echo "test";
    }

    if (maybe_int($input)) {
        echo "test";
    }

    if (!maybe_int($input)) {
        echo "test";
    }

    if (!!$nullable_x) {
        echo "test";
    }

    if ($input && $nullable_x || !$nullable_y) {
        echo "test";
    }

    return $nullable_x ? 5 : 4;

    return maybe_object($input) ? 5 : 4;
}
