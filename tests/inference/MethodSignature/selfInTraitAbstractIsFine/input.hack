trait SomeTrait {
    abstract public function a(SomeClass $b): SomeClass;
}

final class SomeClass {
    use SomeTrait;

    <<__Override>>
    public function a(SomeClass $b): SomeClass {
        return $this;
    }
}