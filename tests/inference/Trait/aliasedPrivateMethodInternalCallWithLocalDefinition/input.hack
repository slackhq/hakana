trait T1 {
    use T2;

    private function foo() : int {
        return $this->bar();
    }
}

trait T2 {
    private function bar() : int {
        return 3;
    }
}

final class A {
    use T1;

    private function baz() : int {
        return $this->bar();
    }
}