interface I {
    public function boo() : void;
}

trait T {
    private function boo() : void {}
}

final class A implements I {
    use T;
}