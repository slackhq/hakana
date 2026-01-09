final class A {
    public function test(): void {}
}

final class B {
    private ?A $maybe_null = null;

    public function __construct(private A $never_null) {}

    public function call(): void {
        $this->maybe_null?->test();
        $this->never_null?->test();

        $this->maybe_null->test();
        $this->never_null->test();
    }
}
