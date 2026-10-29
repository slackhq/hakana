final class Bounded<T, TC as Traversable<T>> {
    public function __construct(public T $value) {}
    public function accept(TC $values): void {}
}

function make<T>(T $value): Bounded<T, vec<T>> {
    return new Bounded($value);
}
