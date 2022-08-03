trait T {
    private function bar() : void {}
}

class A {
    use T {
        bar as protected;
    }
}

class AChild extends A {
    public function foo() : void {
        $this->bar();
    }
}