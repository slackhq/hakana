class A {
    public dict<arraykey, mixed> $arr = dict[];

    public function foo() : void {
        if (rand(0, 1)) {
            $this->arr["a"] = "hello";
        }

        if (!$this->arr) {}
    }
}