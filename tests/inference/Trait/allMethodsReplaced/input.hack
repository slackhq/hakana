trait T {
    protected function foo() : void {}

    public function bat() : void {
        $this->foo();
    }
}

final class C {
    use T;

    protected function foo(string $s) : void {}

    public function bat() : void {
        $this->foo("bat");
    }
}