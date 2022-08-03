trait T {
    public function bar() : int {
        return 3;
    }
}

class A {
    use T {
        bar as bat;
    }

    public function bar() : string {
        return "hello";
    }

    public function baz() : string {
        return $this->bar();
    }
}