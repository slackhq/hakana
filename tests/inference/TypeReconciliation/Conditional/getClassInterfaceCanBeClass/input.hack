interface Id {}

final class A {
    public function is(Id $other): bool {
        return get_class($this) === get_class($other);
    }
}