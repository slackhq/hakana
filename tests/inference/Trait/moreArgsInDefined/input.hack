trait T {
    abstract public function foo() : void;

    public function callFoo() : void {
        $this->foo();
    }
}

class A {
    use T;

    public function foo(string $s = null) : void {

    }
}