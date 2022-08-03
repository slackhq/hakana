trait T {
    public function foo() : int {
        return $this->bar();
    }

    public function bar() : int {
        return 3;
    }
}

class A {
    use T {
        bar as bat;
    }

    public function baz() : int {
        return $this->bar();
    }
}