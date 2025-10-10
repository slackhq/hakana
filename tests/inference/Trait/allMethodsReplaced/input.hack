trait T {
    protected function foo() : void {}

    public function bat() : void {
        $this->foo();
    }
}

final class C {
    use T;

    <<__Override>>
    protected function foo(string $s) : void {}

    <<__Override>>
    public function bat() : void {
        $this->foo("bat");
    }
}