trait T {
    private function bar() : void {}
}

class A {
    use T;
}

class AChild extends A {
    public function foo() : void {
        $this->bar();
    }
}