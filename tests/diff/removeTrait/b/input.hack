trait T {
    public function foo(): void {}
}

class C {
}

function foo(C $c) {
    $c->foo();
}

foo(new C());