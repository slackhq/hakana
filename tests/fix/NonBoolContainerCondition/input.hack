type input_t = shape(
    ?'container' => \HH\Container
);

function returns_nullable(bool $b): ?dict<string, int> {
    return $b ? dict[] : null;
}

function main(?\HH\Container $nc, \HH\Container $c, input_t $input, bool $b): void {
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

    if (returns_nullable()) {
        echo "foo";
    }

    if (returns_nullable() ?? dict[]) {
        echo "foo";
    }
}
