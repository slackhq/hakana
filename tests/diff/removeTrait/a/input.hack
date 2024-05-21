trait T {
    public function foo(): void {}
}

final class C {
    use T;
}

function foo(C $c) {
    $c->foo();
}

foo(new C());