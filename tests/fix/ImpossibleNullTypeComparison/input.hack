final class B {
    public function foo(): void {}
}

function foo(): void {
    $b = new B();
    $b?->foo();
}

function bar(?B $b): void {
    $b?->foo();
}
