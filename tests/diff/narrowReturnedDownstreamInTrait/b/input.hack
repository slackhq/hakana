<<__EntryPoint>>
function foo(): void {
    $a = new B();
    $b = $a->bar();
    if ($b is string) {}
}

abstract class A {
    public function bar(): arraykey {
        return 10;
    }
}

trait T {
    public function bar(): string {
        return "hello";
    }
}

final class B extends A {
    use T;
}
