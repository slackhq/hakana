trait T {
    public function foo(): void {}
}

final class C {
}

function foo(C $c) {
    $c->foo();
}