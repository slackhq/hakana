abstract class SomeParent<+T> {
    public function __construct(public T $t) {}
}

class SomeChild<+T> extends SomeParent<T> {}

function foo(
    vec<shape('id' => string)> $vec,
    shape('name' => string) $shape
): SomeParent<(Container<shape('id' => string)>, shape('name' => string))> {
    return new SomeChild(tuple($vec, $shape));
}