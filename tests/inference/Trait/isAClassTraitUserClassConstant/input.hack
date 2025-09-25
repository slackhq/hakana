trait T {
    public function f(): void {
        if (is_a(static::class, nameof B, true)) { }
    }
}

final class A {
    use T;
}

final class B {
    use T;

    public function foo() : void {}
}