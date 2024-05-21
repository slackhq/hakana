final class Foo {
    private ?Foo $nullableSelf = null;

    public function __construct(private Foo $self) {}

    public function doBar(): ?Foo
    {
        return $this->nullableSelf?->self;
    }
}