trait T {
    public function foo(): void {}
}

class C {
    use T;
}

function foo(C $c) {
    $c->foo();
}

foo(new C());