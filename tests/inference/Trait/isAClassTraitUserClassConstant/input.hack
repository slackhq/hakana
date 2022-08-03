trait T {
    public function f(): void {
        if (is_a(static::class, B::class, true)) { }
    }
}

class A {
    use T;
}

class B {
    use T;

    public function foo() : void {}
}