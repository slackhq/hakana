interface Stringable {
    public function __toString() {}
}
function foo(Stringable $s): void {}

final class Bar {
    public function __toString() {
        return "foo";
    }
}

foo(new Bar());