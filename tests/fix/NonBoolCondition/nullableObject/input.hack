function some_cond(int $i): bool {
    return $i++ % 2 === 1;
}

final class C {}

function maybe_object(bool $test): ?C {
    return $test ? new C() : null;
}

function main(bool $input, int $x, string $foo): int {
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

    invariant(maybe_object($input), "invariant violation");

    return maybe_object($input) ? 5 : 4;
}

function newly_supported_types(
    ?(function(): void) $closure,
    ?classname<C> $classname,
    ?class<C> $class_ptr,
    ?typename<C> $typename,
    ?Awaitable<int> $awaitable,
    ?shape('value' => int) $shape,
    ?(int, string) $tuple,
): void {
    if (!$closure) {
        echo "test";
    }

    if ($classname) {
        echo "test";
    }

    if (!$class_ptr) {
        echo "test";
    }

    if ($typename) {
        echo "test";
    }

    if (!$awaitable) {
        echo "test";
    }

    if ($shape) {
        echo "test";
    }

    if (!$tuple) {
        echo "test";
    }
}
