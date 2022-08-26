function foo(shape('a' => ?vec<shape('b' => string)>) $shape): void {}

function bar($m): string {
    $vec = rand(0, 1) ? vec($m) : null;

    foo(shape('a' => $vec));
}