abstract class A {
    public function method(): this {
        return $this;
    }
}
final class B extends A {
    <<__Override>>
    public function method(): this {
        return $this;
    }
}