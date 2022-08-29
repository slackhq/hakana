type some_shape = shape('a' => int, 'b' => string);

function foo(dict<string, mixed> $dict): some_shape {
    return $dict |> $$ as some_shape;
}