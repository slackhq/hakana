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

function baz(?vec<mixed> $v): void {
    $vv = $v ?? vec[];
    $obj = $vv[0] ?? null;
    $obj?->foo();
}
