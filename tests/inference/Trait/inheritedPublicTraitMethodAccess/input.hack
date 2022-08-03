trait T {
    private function bar() : void {}
}

class A {
    use T {
        bar as public;
    }
}

(new A)->bar();