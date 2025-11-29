final class MyClass {
    public function getValue(): int {
        return 42;
    }
}

final class Container {
    const MyClass DEFAULT_VALUE = new MyClass();
    const int MAX_COUNT = 100;
}
