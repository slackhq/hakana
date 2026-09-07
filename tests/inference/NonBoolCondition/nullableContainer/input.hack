type input_t = shape(
    ?'container' => \HH\Container
);
function main(?\HH\Container $nc, \HH\Container $c, input_t $input, bool $b, ?input_t $foo, ?shape(?'foo' => string) $bar = null): void {
    if ($nc) {
        echo "test";
    }

    if (!$c || !$b) {
        echo "test";
    }

    if ($c && $b) {
        echo "foo";
    }

    if ($nc ?? dict[]) {
        echo "test";
    }

    if ($b && !$nc) {
        echo "foo";
    }

    if (!($input['container'] ?? null)) {
        echo "test";
    }

    if ($foo) {
        echo "test";
    }

    if (returns_tuple($b)) {
        echo "test";
    }

    if ($bar && Shapes::keyExists($bar, 'foo')) {
        echo "test";
    }

    if ($bar) {
        echo "test";
    }
}

function returns_tuple(bool $b): ?(int, string) {
    return $b ? tuple(5, "a") : null;
}
