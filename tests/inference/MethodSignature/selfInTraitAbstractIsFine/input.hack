trait SomeTrait {
    abstract public function a(SomeClass $b): SomeClass;
}

final class SomeClass {
    use SomeTrait;

    public function a(SomeClass $b): SomeClass {
        return $this;
    }
}