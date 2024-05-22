interface I {
    public function fooBar(): vec<string>;
}

abstract class A implements I
{
    public function g(): vec<string> {
        return $this->fooBar();
    }
}