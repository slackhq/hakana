trait T {
    public function foo(): void {}
}

final class C {
    use T;
}

function foo(C $c) {
    $c->foo();
}

<<__EntryPoint>>
function main(): void {
    foo(new C());
}