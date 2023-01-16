class A {
    public function foo(string $s): ?string {
        return rand(0, 1) ? $s : null;
    }
}

class B extends A {
    public function foo(?string $s): string {
        return $s ?: "hello";
    }
}

echo (new B())->foo(null);