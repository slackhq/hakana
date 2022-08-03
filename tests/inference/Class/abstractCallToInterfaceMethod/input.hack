interface I {
    public function fooBar(): array;
}

abstract class A implements I
{
    public function g(): array {
        return $this->fooBar();
    }
}