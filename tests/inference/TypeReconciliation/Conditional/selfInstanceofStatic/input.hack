class A {
    public function foo(A $value): void {
        if ($value is this) {}
    }
}