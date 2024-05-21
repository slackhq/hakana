trait T {
    private function bar() : void {}
}

abstract class A {
    use T;
}

final class AChild extends A {
    public function foo() : void {
        $this->bar();
    }
}