interface I {
    public function boo() : void;
}

trait T {
    private function boo() : void {}
}

class A implements I {
    use T { boo as public; }
}