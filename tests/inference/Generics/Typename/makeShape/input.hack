function randomShape<T as shape(...)>(typename<T> $t): T {
    throw new \Exception('bad');
}

type foo = shape("id" => int);

function bar(): foo {
    return randomShape(foo::class);
}