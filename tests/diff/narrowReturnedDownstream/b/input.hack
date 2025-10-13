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

final class B extends A {
    <<__Override>>
    public function bar(): string {
        return "hello";
    }
}