class A {
    public function method(): this {
        return $this;
    }
}
class B extends A {
    public function method(): this {
        return $this;
    }
}