interface I {
    public function foo() : void;
}

abstract class A implements I {
    public function bar(): void {
        $this->foo();
    }
}