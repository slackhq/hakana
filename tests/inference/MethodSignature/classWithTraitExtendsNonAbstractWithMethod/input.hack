class A {
    public function foo() : void {}
}

trait T {
    abstract public function foo() : void;
}

class B extends A {
    use T;
}