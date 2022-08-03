class A {
    private function __clone() {}
    public function foo(): A {
        return clone $this;
    }
}