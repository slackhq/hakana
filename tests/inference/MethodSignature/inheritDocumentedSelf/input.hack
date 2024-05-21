interface I {
    public function foo(I $f) : I;
}

final class C implements I {
    public function foo(I $f) : I {
        return new C();
    }
}