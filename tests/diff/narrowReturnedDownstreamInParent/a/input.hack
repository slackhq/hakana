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
}

final class C extends B {}