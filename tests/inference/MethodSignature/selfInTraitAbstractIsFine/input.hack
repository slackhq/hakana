trait SomeTrait {
    abstract public function a(SomeClass $b): SomeClass;
}

class SomeClass {
    use SomeTrait;

    public function a(SomeClass $b): SomeClass {
        return $this;
    }
}