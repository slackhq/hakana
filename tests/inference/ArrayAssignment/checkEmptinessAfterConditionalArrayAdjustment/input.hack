final class A {
    public dict<arraykey, mixed> $arr = dict[];

    public function foo() : void {
        if (rand(0, 1) !== 0) {
            $this->arr["a"] = "hello";
        }

        if (C\is_empty($this->arr)) {}
    }
}
