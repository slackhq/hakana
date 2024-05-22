<<__EntryPoint>>
function foo(): void {
    $a = new C();
    $b = $a->bar();
    if ($b is string) {}
}

abstract class A {
    public function bar(): arraykey {
        return 10;
    }
}

abstract class B extends A {
    public function bar(): string {
        return "hello";
    }
}

final class C extends B {}