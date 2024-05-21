final class A {
    public dict<arraykey, mixed> $foo = dict[];
    public dict<arraykey, mixed> $bar = dict[];

    public function setter() : void {
        if ($this->foo) {
            $this->foo = dict[];
        }
    }

    public function iffer() : bool {
        return $this->foo || $this->bar;
    }
}