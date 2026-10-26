final class A {
    public dict<arraykey, mixed> $foo = dict[];
    public dict<arraykey, mixed> $bar = dict[];

    public function setter() : void {
        if (!C\is_empty($this->foo)) {
            $this->foo = dict[];
        }
    }

    public function iffer() : bool {
        return !C\is_empty($this->foo) || !C\is_empty($this->bar);
    }
}