interface I {
    public function boo() : void;
}

trait T {
    private function boo() : void {}
}

class B {
    use T { boo as public; }
}

class BChild extends B implements I {}