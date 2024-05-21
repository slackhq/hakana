final class A {
    public string $a = "";
    public string $b = "";

    public function fooFoo(): string
    {
        list($this->a, $this->b) = vec["a", "b"];

        return $this->a;
    }
}