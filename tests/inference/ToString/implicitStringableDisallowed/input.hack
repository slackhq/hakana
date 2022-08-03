interface Stringable {
    function __toString() {}
}
function foo(Stringable $s): void {}

class Bar {
    public function __toString() {
        return "foo";
    }
}

foo(new Bar());