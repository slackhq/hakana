type input_t = shape('foo'=>string);
type aggregate_t = shape(
    'foo' => string,
    'count' => int,
);
function test(vec<input_t> $input, string $foo): aggregate_t {
    $initial_value = shape(
        'foo' => $foo,
        'count' => 1,
    );

    return C\reduce(
        $input,
        ($acc, $engagement) ==> shape(
            'foo' => $acc['foo'],
            'count' => $acc['count'] + 1,
        ),
        $initial_value,
    );
}


function test2(vec<input_t> $input): (vec<input_t>, dict<nothing, nothing>) {
    return C\reduce(
        $input,
        ($acc, $item) ==> {
            list($bar, $baz) = $acc;
            $bar[] = $item;
            return tuple($bar, $baz);
        },
        tuple(vec[], dict[])
    );
}


function bad(vec<input_t> $input): (vec<input_t>, dict<nothing, nothing>) {
    return C\reduce(
        $input,
        ($acc, $item) ==> {
            list($bar, $baz) = $acc;
            $bar[] = $item;
            // swapped
            return tuple($baz, $bar);
        },
        tuple(vec[], dict[])
    );
}
