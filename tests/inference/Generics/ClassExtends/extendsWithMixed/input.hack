abstract class SomeParent<T> {
    public function __construct(public T $t) {}
}

class SomeChild<T> extends SomeParent<T> {}

function foo($m): SomeParent<string> {
    return new SomeChild($m);
}