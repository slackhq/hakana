class A {
    public function foo(string $s): string {
        return $s;
    }
}

class B extends A {
    public function foo(string $s = null): string {
        return $s ?: "hello";
    }
}

echo (new B())->foo();