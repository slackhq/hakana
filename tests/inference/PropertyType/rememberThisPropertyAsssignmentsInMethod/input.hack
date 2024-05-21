final class A {
    public bool $foo = false;

    public function bar() : void {
        $this->foo = false;
        $this->maybeChange();

        if ($this->foo) {}
    }

    public function maybeChange() : void {
        if (rand(0, 1)) {
            $this->foo = true;
        }
    }
}