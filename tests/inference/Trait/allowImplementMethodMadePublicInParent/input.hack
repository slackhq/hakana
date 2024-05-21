interface I {
    public function boo() : void;
}

trait T {
    private function boo() : void {}
}

abstract class B {
    use T;
}

final class BChild extends B implements I {}