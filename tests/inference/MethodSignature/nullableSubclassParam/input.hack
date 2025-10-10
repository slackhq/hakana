abstract class A {
    public function foo(string $s): ?string {
        return rand(0, 1) ? $s : null;
    }
}

final class B extends A {
    <<__Override>>
    public function foo(?string $s): string {
        return $s ?: "hello";
    }
}

echo (new B())->foo(null);