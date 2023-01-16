interface I {
    public function boo() : void;
}

trait T {
    private function boo() : void {}
}

class B {
    use T;
}

class BChild extends B implements I {}