<<__EntryPoint>>
function foo(): void {
    bar();
    $b = new B();
    $b->baz();
    $b->bat();
    throw new A();
}

function bar(): void {
    echo A::class;
}

final class B {
    public function baz(): void {
        echo A::class;
    }

    public function bat(): int {
        // deliberate error that should only appear once
        return "a";
    }
}